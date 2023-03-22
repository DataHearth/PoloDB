/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::cell::RefCell;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use byteorder::{ReadBytesExt, WriteBytesExt};
use crc64fast::Digest;
use getrandom::getrandom;
use crate::{Config, DbErr, DbResult};
use crate::lsm::lsm_snapshot::LsmSnapshot;
use crate::lsm::mem_table::MemTable;

static HEADER_DESP: &str       = "PoloDB Journal v0.4";
const DATABASE_VERSION: [u8; 4] = [0, 0, 4, 0];
const DATA_BEGIN_OFFSET: u64 = 64;

enum LogCommand {
    Insert(Box<[u8]>, Vec<u8>),
    Delete(Box<[u8]>)
}

#[allow(dead_code)]
mod format {
    pub const EOF: u8     = 0x00;
    pub const PAD1: u8    = 0x01;
    pub const PAD2: u8    = 0x02;
    pub const COMMIT: u8  = 0x03;
    pub const JUMP: u8    = 0x04;
    pub const WRITE: u8   = 0x06;
    pub const DELETE: u8  = 0x08;
}

struct LogTransactionState {
    digest: Digest,
}

impl LogTransactionState {

    fn new() -> LogTransactionState {
        LogTransactionState {
            digest: Digest::new(),
        }
    }

}

pub(crate) struct LsmCommitResult {
    pub offset: u64,
}

pub(crate) struct LsmLog {
    inner: RefCell<LsmLogInner>
}

impl LsmLog {

    pub fn open(path: &Path, config: Config) -> DbResult<LsmLog> {
        let inner = LsmLogInner::open(path, config)?;
        Ok(LsmLog {
            inner: RefCell::new(inner),
        })
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> DbResult<()> {
        let mut inner = self.inner.borrow_mut();
        inner.put(key, value)
    }

    pub fn delete(&self, key: &[u8]) -> DbResult<()> {
        let mut inner = self.inner.borrow_mut();
        inner.delete(key)
    }

    pub fn start_transaction(&self) -> DbResult<()> {
        let mut inner = self.inner.borrow_mut();
        inner.start_transaction()
    }

    pub fn rollback(&self) -> DbResult<()> {
        let mut inner = self.inner.borrow_mut();
        inner.rollback()
    }

    pub fn commit(&self) -> DbResult<LsmCommitResult> {
        let mut inner = self.inner.borrow_mut();
        inner.commit()
    }

    pub fn update_mem_table_with_latest_log(
        &self,
        snapshot: &LsmSnapshot,
        mem_table: &mut MemTable,
    ) -> DbResult<()> {
        let mut inner = self.inner.borrow_mut();
        inner.update_mem_table_with_latest_log(snapshot, mem_table)
    }

}

fn generate_a_salt() -> u32 {
    let mut buf: [u8; 4] = [0; 4];
    getrandom(&mut buf).unwrap();
    u32::from_le_bytes(buf)
}

fn crc64(bytes: &[u8]) -> u64 {
    let mut c = Digest::new();
    c.write(bytes);
    c.sum64()
}

struct LsmLogInner {
    file_path:   PathBuf,
    file:        File,
    transaction: Option<LogTransactionState>,
    offset:      u64,
    salt1:       u32,
    salt2:       u32,
    config:      Config,
}

impl LsmLogInner {

    fn open(path: &Path, config: Config) -> DbResult<LsmLogInner> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(path)?;
        let meta = file.metadata()?;

        let file_path: PathBuf = path.to_path_buf();
        let mut result = LsmLogInner {
            file_path,
            file,
            transaction: None,
            offset: 0,
            salt1: generate_a_salt(),
            salt2: generate_a_salt(),
            config,
        };

        if meta.len() == 0 {
            result.force_init_header()?;
        } else {
            result.read_and_check_from_file()?;
        }

        Ok(result)
    }

    /// name:       32 bytes
    /// version:    4bytes(offset 32)
    /// page_size:  4bytes(offset 36)
    /// salt_1:     4bytes(offset 40)
    /// salt_2:     4bytes(offset 44)
    /// checksum before 48:   8bytes(offset 48)
    /// data begin: 64 bytes
    fn force_init_header(&mut self) -> DbResult<()> {
        let mut header48: Vec<u8> = vec![];
        header48.resize(48, 0);

        // copy title
        let title_bytes = HEADER_DESP.as_bytes();
        header48[0..title_bytes.len()].copy_from_slice(title_bytes);

        // copy version
        header48[32..36].copy_from_slice(&DATABASE_VERSION);
        let page_size_be = self.config.get_lsm_page_size().to_be_bytes();
        header48[36..40].copy_from_slice(&page_size_be);

        let salt_1_be = self.salt1.to_be_bytes();
        header48[40..44].copy_from_slice(&salt_1_be);

        let salt_2_be = self.salt2.to_be_bytes();
        header48[44..48].copy_from_slice(&salt_2_be);

        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header48)?;

        let checksum = crc64(&header48);
        let checksum_be = checksum.to_be_bytes();

        self.file.seek(SeekFrom::Start(48))?;
        self.file.write_all(&checksum_be)?;

        self.file.flush()?;

        self.file.set_len(DATA_BEGIN_OFFSET)?;
        self.offset = self.file.seek(SeekFrom::End(0))?;

        Ok(())
    }

    fn read_and_check_from_file(&mut self) -> DbResult<()> {
        let mut header48: Vec<u8> = vec![0; 48];
        self.file.read_exact(&mut header48)?;

        let checksum = crc64(&header48);
        let checksum_from_file = self.read_checksum_from_file()?;
        if checksum != checksum_from_file {
            return Err(DbErr::ChecksumMismatch);
        }

        // // copy version
        // self.version.copy_from_slice(&header48[32..36]);

        let mut buffer: [u8; 4] = [0; 4];
        buffer.copy_from_slice(&header48[40..44]);
        self.salt1 = u32::from_be_bytes(buffer);

        let mut buffer: [u8; 4] = [0; 4];
        buffer.copy_from_slice(&header48[44..48]);
        self.salt2 = u32::from_be_bytes(buffer);

        self.offset = DATA_BEGIN_OFFSET;
        self.file.seek(SeekFrom::Start(DATA_BEGIN_OFFSET))?;

        Ok(())
    }

    // TODO: this is very slow
    fn checksum_from(&mut self, from: u64, to: u64) -> DbResult<u64> {
        let mut digest= Digest::new();

        let mut buffer = vec![0u8; (to - from) as usize];

        self.file.seek(SeekFrom::Start(from))?;
        self.file.read_exact(&mut buffer)?;

        digest.write(&buffer);

        Ok(digest.sum64())
    }

    pub fn update_mem_table_with_latest_log(&mut self, snapshot: &LsmSnapshot, mem_table: &mut MemTable) -> DbResult<()> {
        let mut offset = snapshot.log_offset + DATA_BEGIN_OFFSET;
        self.file.seek(SeekFrom::Start(offset))?;

        let mut commands: Vec<LogCommand> = vec![];

        loop {
            let test_read = self.file.read_u8();
            let flag = match test_read {
                Ok(byte) => byte,
                Err(_) => break,
            };


            if flag == format::COMMIT {
                let current_pos = self.file.seek(SeekFrom::Current(0))?;
                let checksum = self.checksum_from(offset, current_pos - 1)?;
                self.file.seek(SeekFrom::Start(current_pos))?;

                let mut checksum_be: [u8; 8] = [0; 8];
                self.file.read_exact(&mut checksum_be)?;
                let expect_checksum = u64::from_be_bytes(checksum_be);
                if checksum != expect_checksum {
                    self.file.set_len(offset)?;
                    self.offset = offset;
                    return Ok(());
                }

                offset = self.file.seek(SeekFrom::Current(0))?;

                LsmLogInner::flush_commands_to_mem_table(commands, mem_table);
                commands = vec![];
            } else if flag == format::WRITE {
                let key_len = crate::btree::vli::decode_u64(&mut self.file)?;
                let mut key_buff = vec![0u8; key_len as usize];
                self.file.read_exact(&mut key_buff)?;

                let value_len = crate::btree::vli::decode_u64(&mut self.file)?;
                let mut value_buff = vec![0u8; value_len as usize];
                self.file.read_exact(&mut value_buff)?;

                commands.push(LogCommand::Insert(key_buff.into_boxed_slice(), value_buff));
            } else if flag == format::DELETE {
                let key_len = crate::btree::vli::decode_u64(&mut self.file)?;
                let mut key_buff = vec![0u8; key_len as usize];
                self.file.read_exact(&mut key_buff)?;

                commands.push(LogCommand::Delete(key_buff.into_boxed_slice()));
            } else {  // unknown command
                self.file.set_len(offset)?;
                self.offset = offset;
                return Ok(());
            }
        }

        Ok(())
    }

    fn flush_commands_to_mem_table(commands: Vec<LogCommand>, mem_table: &mut MemTable) {
        for cmd in commands {
            match cmd {
                LogCommand::Insert(key, value) => {
                    mem_table.segments.insert_in_place(key, value);
                }
                LogCommand::Delete(key) => {
                    mem_table.segments.delete_in_place(key.as_ref());
                }
            }
        }
    }

    fn read_checksum_from_file(&mut self) -> DbResult<u64> {
        self.file.seek(SeekFrom::Start(48))?;
        let mut buffer: [u8; 8] = [0; 8];
        self.file.read_exact(&mut buffer)?;
        Ok(u64::from_be_bytes(buffer))
    }

    fn write_without_checksum(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.file.write(bytes)?;
        self.offset += bytes.len() as u64;
        Ok(())
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) -> DbResult<()> {
        self.write_u8(format::WRITE)?;

        let key_len = key.len();
        crate::btree::vli::encode(self, key_len as i64)?;

        self.write_all(key)?;

        let value_len = value.len();
        crate::btree::vli::encode(self, value_len as i64)?;

        self.write_all(value)?;

        Ok(())
    }

    pub fn delete(&mut self, key: &[u8]) -> DbResult<()> {
        self.write_u8(format::DELETE)?;

        let key_len = key.len();
        crate::btree::vli::encode(self, key_len as i64)?;

        self.write_all(key)?;

        Ok(())
    }

    /// Go to the end of the file
    pub fn start_transaction(&mut self) -> DbResult<()> {
        let state = LogTransactionState::new();
        self.transaction = Some(state);
        self.offset = self.file.seek(SeekFrom::End(0))?;
        Ok(())
    }

    pub fn rollback(&mut self) -> DbResult<()> {
        self.file.set_len(self.offset)?;
        self.transaction = None;
        Ok(())
    }

    pub fn commit(&mut self) -> DbResult<LsmCommitResult> {
        if self.transaction.is_none() {
            return Err(DbErr::NoTransactionStarted);
        }
        {
            let state = self.transaction.as_ref().unwrap();
            let checksum = state.digest.sum64();
            let checksum_be: [u8; 8] = checksum.to_be_bytes();

            self.write_without_checksum(&[format::COMMIT])?;
            self.write_without_checksum(&checksum_be)?;
        }
        self.transaction = None;
        self.file.flush()?;
        self.offset = self.file.seek(SeekFrom::End(0))?;

        Ok(LsmCommitResult {
            offset: self.offset - DATA_BEGIN_OFFSET,
        })
    }

}

impl Write for LsmLogInner {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.transaction.is_none() {
            return Err(std::io::ErrorKind::NotFound.into());
        }
        let state = self.transaction.as_mut().unwrap();
        state.digest.write(buf);

        self.write_without_checksum(buf)?;

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
