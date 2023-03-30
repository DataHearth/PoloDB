/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use crate::{Config, DbErr, DbResult};
use crate::lsm::kv_cursor::KvCursor;
use crate::lsm::lsm_segment::LsmTuplePtr;
use crate::lsm::LsmMetrics;
use crate::lsm::lsm_snapshot::LsmSnapshot;
use super::lsm_backend::{LsmFileBackend, LsmLog};
use crate::lsm::mem_table::MemTable;
use crate::lsm::multi_cursor::{CursorRepr, MultiCursor};

#[derive(Clone)]
pub struct LsmKv {
    inner: Arc<LsmKvInner>,
}

impl LsmKv {

    pub fn open_file(path: &Path) -> DbResult<LsmKv> {
        let config = Config::default();
        LsmKv::open_file_with_config(path, config)
    }

    pub fn open_file_with_config(path: &Path, config: Config) -> DbResult<LsmKv> {
        let inner = LsmKvInner::open_file(path, config)?;
        LsmKv::open_with_inner(inner)
    }

    pub fn open_memory() -> DbResult<LsmKv> {
        LsmKv::open_memory_with_config(Config::default())
    }

    pub fn open_memory_with_config(config: Config) -> DbResult<LsmKv> {
        let metrics = LsmMetrics::new();
        let inner = LsmKvInner::open_with_backend(None, None, metrics, config)?;
        LsmKv::open_with_inner(inner)
    }

    #[inline]
    fn open_with_inner(inner: LsmKvInner) -> DbResult<LsmKv> {
        Ok(LsmKv {
            inner: Arc::new(inner),
        })
    }

    pub fn open_cursor(&self) -> KvCursor {
        let multi_cursor = self.inner.open_multi_cursor();
        KvCursor::new(self.inner.clone(), multi_cursor)
    }

    pub fn put<K, V>(&self, key: K, value: V) -> DbResult<()>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.inner.start_transaction()?;
        self.inner.put(key.as_ref(), value.as_ref())?;
        self.inner.commit()
    }

    pub fn delete<K>(&self, key: K) -> DbResult<()>
    where
        K: AsRef<[u8]>
    {
        self.inner.start_transaction()?;
        self.inner.delete(key.as_ref())?;
        self.inner.commit()
    }

    pub fn get<'a, K>(&self, key: K) -> DbResult<Option<Vec<u8>>>
    where
        K: AsRef<[u8]>,
    {
        let cursor = self.open_cursor();
        cursor.seek(key.as_ref())?;
        let test_key = cursor.key()?;

        match test_key {
            Some(test_key) => {
                if test_key.as_ref().cmp(key.as_ref()) != std::cmp::Ordering::Equal {
                    return Ok(None);
                }
            }
            None => return Ok(None),
        };

        let value = cursor.value()?;
        let result = match value {
            Some(bytes) => Some(bytes),
            None => None,
        };
        Ok(result)
    }

    pub fn get_string<'a, K>(&self, key: K) -> DbResult<Option<String>>
        where
            K: AsRef<[u8]>,
    {
        let bytes = self.get(key)?;
        let string = match bytes {
            None => None,
            Some(bytes) => {
                let result = String::from_utf8(bytes)?;
                Some(result)
            }
        };
        Ok(string)
    }

    pub fn metrics(&self) -> LsmMetrics {
        self.inner.metrics()
    }

}

pub(crate) struct LsmKvInner {
    backend: Option<Box<LsmFileBackend>>,
    log: Option<LsmLog>,
    snapshot: Mutex<LsmSnapshot>,
    mem_table: RefCell<MemTable>,
    in_transaction: Cell<bool>,
    /// Operation count after last sync,
    /// including insert/delete
    op_count: AtomicUsize,
    metrics: LsmMetrics,
    pub(crate) config: Config,
}

impl LsmKvInner {

    pub(crate) fn read_segment_by_ptr(&self, ptr: LsmTuplePtr) -> DbResult<Vec<u8>> {
        let backend = self.backend.as_ref().expect("no file backend");
        backend.read_segment_by_ptr(ptr)
    }

    fn mk_log_path(db_path: &Path) -> PathBuf {
        let mut buf = db_path.to_path_buf();
        let filename = buf.file_name().unwrap().to_str().unwrap();
        let new_filename = String::from(filename) + ".wal";
        buf.set_file_name(new_filename);
        buf
    }

    fn open_file(path: &Path, config: Config) -> DbResult<LsmKvInner> {
        let metrics = LsmMetrics::new();
        let backend = LsmFileBackend::open(path, metrics.clone(), config.clone())?;
        let log_file = LsmKvInner::mk_log_path(path);
        let log = LsmLog::open(log_file.as_path(), config.clone())?;
        LsmKvInner::open_with_backend(
            Some(Box::new(backend)),
            Some(log),
            metrics,
            config,
        )
    }

    fn open_with_backend(
        backend: Option<Box<LsmFileBackend>>,
        log: Option<LsmLog>,
        metrics: LsmMetrics,
        config: Config,
    ) -> DbResult<LsmKvInner> {
        let snapshot = match &backend {
            Some(backend) => backend.read_latest_snapshot()?,
            None => LsmSnapshot::new(),
        };
        let mut mem_table = MemTable::new(0);

        if let Some(log) = &log {
            log.update_mem_table_with_latest_log(
                &snapshot,
                &mut mem_table,
            )?;
        }

        Ok(LsmKvInner {
            backend,
            log,
            snapshot: Mutex::new(snapshot),
            mem_table: RefCell::new(mem_table),
            in_transaction: Cell::new(false),
            op_count: AtomicUsize::new(0),
            metrics,
            config,
        })
    }

    #[inline]
    fn metrics(&self) -> LsmMetrics {
        self.metrics.clone()
    }

    fn open_multi_cursor(&self) -> MultiCursor {
        let mem_table = self.mem_table.borrow();
        let mem_table_cursor = mem_table.open_cursor();

        let snapshot = self.snapshot.lock().unwrap();

        let mut cursors: Vec<CursorRepr> = vec![
            mem_table_cursor.into(),
        ];

        if !snapshot.levels.is_empty() {
            // push all cursor on level 0
            let level0 = &snapshot.levels[0];

            for item in level0.content.iter().rev() {
                let cursor = item.segments.open_cursor();
                cursors.push(cursor.into());
            }

            for level in &snapshot.levels[1..] {
                assert_eq!(level.content.len(), 1);
                let cursor = level.content[0].segments.open_cursor();
                cursors.push(cursor.into());
            }
        }

        MultiCursor::new(cursors)
    }

    fn start_transaction(&self) -> DbResult<()> {
        if let Some(log) = &self.log {
            log.start_transaction()?;
        }

        self.in_transaction.set(true);

        Ok(())
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> DbResult<()> {
        if !self.in_transaction.get() {
            return Err(DbErr::NoTransactionStarted);
        }

        if let Some(log) = &self.log {
            log.put(key, value)?;
        }

        let mut segment = self.mem_table.borrow_mut();

        segment.put(key, value);

        self.op_count.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    pub fn delete(&self, key: &[u8]) -> DbResult<()> {
        if !self.in_transaction.get() {
            return Err(DbErr::NoTransactionStarted);
        }

        if let Some(log) = &self.log {
            log.delete(key)?;
        }

        let mut segment = self.mem_table.borrow_mut();

        segment.delete(key);

        self.op_count.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    fn commit(&self) -> DbResult<()> {
        if !self.in_transaction.get() {
            return Err(DbErr::NoTransactionStarted);
        }

        if let Some(log) = &self.log {
            let _commit_result = log.commit()?;
            // let mut snapshot = self.snapshot.lock()?;
            // snapshot.log_offset = commit_result.offset;
        }

        if let Some(backend) = &self.backend {
            let mut mem_table = self.mem_table.borrow_mut();
            let mut snapshot = self.snapshot.lock()?;

            let store_bytes = mem_table.store_bytes();
            if self.should_sync(store_bytes) {
                backend.sync_latest_segment(
                    &mut mem_table,
                    &mut snapshot,
                )?;
                backend.checkpoint_snapshot(&mut snapshot)?;

                if let Some(log) = &self.log {
                    log.shrink(&mut snapshot)?;
                }

                mem_table.clear();

                self.op_count.store(0, Ordering::Relaxed);
                self.metrics.add_sync_count();
            } else if LsmKvInner::should_minor_compact(&snapshot) {
                self.minor_compact(backend, &mut snapshot)?;
            }
        }

        self.in_transaction.set(false);

        Ok(())
    }

    fn minor_compact(&self, backend: &LsmFileBackend, snapshot: &mut LsmSnapshot) -> DbResult<()> {
        backend.minor_compact(snapshot)?;
        backend.checkpoint_snapshot(snapshot)?;

        self.metrics.add_minor_compact();

        Ok(())
    }

    #[inline]
    fn should_sync(&self, store_bytes: usize) -> bool {
        if self.op_count.load(Ordering::Relaxed) >= 1000 {
            return true;
        }

        let block_size = self.config.get_lsm_block_size();
        return store_bytes > (block_size as usize);
    }

    #[inline]
    fn should_minor_compact(snapshot: &LsmSnapshot) -> bool {
        if snapshot.levels.is_empty() {
            return false;
        }
        let level0 = &snapshot.levels[0];
        level0.content.len() > 4
    }

    pub(crate) fn meta_id(&self) -> u64 {
        let snapshot = self.snapshot.lock().unwrap();
        snapshot.meta_id
    }

}
