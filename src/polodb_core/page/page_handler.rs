use std::fs::File;
use super::page::RawPage;
use super::pagecache::PageCache;
use super::header_page_utils;
use crate::journal::JournalManager;
use crate::DbResult;
use crate::error::DbErr;

static DB_INIT_BLOCK_COUNT: u32 = 16;

pub(crate) struct PageHandler {
    file:                     File,

    pub last_commit_db_size:  u64,

    pub page_size:            u32,
    page_count:               u32,
    page_cache:               PageCache,
    journal_manager:          Box<JournalManager>,
}

impl PageHandler {

    fn read_first_block(file: &mut File, page_size: u32) -> std::io::Result<RawPage> {
        let mut raw_page = RawPage::new(0, page_size);
        raw_page.read_from_file(file, 0)?;
        Ok(raw_page)
    }


    fn force_write_first_block(file: &mut File, page_size: u32) -> std::io::Result<RawPage> {
        let mut raw_page = RawPage::new(0, page_size);
        header_page_utils::init(&mut raw_page);
        raw_page.sync_to_file(file, 0)?;
        Ok(raw_page)
    }

    fn init_db(file: &mut File, page_size: u32) -> std::io::Result<(RawPage, u32)> {
        let meta = file.metadata()?;
        let file_len = meta.len();
        if file_len < page_size as u64 {
            file.set_len((page_size as u64) * (DB_INIT_BLOCK_COUNT as u64))?;
            let first_page = PageHandler::force_write_first_block(file, page_size)?;
            Ok((first_page, DB_INIT_BLOCK_COUNT as u32))
        } else {
            let block_count = file_len / (page_size as u64);
            let first_page = PageHandler::read_first_block(file, page_size)?;
            Ok((first_page, block_count as u32))
        }
    }

    pub fn new(path: &str, page_size: u32) -> DbResult<PageHandler> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(path)?;

        let (_, page_count) = PageHandler::init_db(&mut file, page_size)?;

        let journal_file_path: String = format!("{}.journal", &path);
        let journal_manager = JournalManager::open(&journal_file_path, page_size)?;

        let page_cache = PageCache::new_default(page_size);

        let last_commit_db_size = {
            let meta = file.metadata()?;
            meta.len()
        };

        Ok(PageHandler {
            file,

            last_commit_db_size,

            page_size,
            page_count,
            page_cache,
            journal_manager: Box::new(journal_manager),
        })
    }

    // 1. write to journal, if success
    //    - 2. checkpoint journal, if full
    // 3. write to page_cache
    pub fn pipeline_write_page(&mut self, page: &RawPage) -> Result<(), DbErr> {
        self.journal_manager.as_mut().append_raw_page(page)?;

        if self.is_journal_full() {
            self.checkpoint_journal()?;
            #[cfg(feature = "log")]
            eprintln!("checkpoint journal finished");
        }

        self.page_cache.insert_to_cache(page);
        Ok(())
    }

    // 1. read from page_cache, if none
    // 2. read from journal, if none
    // 3. read from main db
    pub fn pipeline_read_page(&mut self, page_id: u32) -> Result<RawPage, DbErr> {
        match self.page_cache.get_from_cache(page_id) {
            Some(page) => {
                #[cfg(feature = "log")]
                eprintln!("read page from cache, page_id: {}", page_id);

                return Ok(page);
            },
            None => (), // nothing
        }

        match self.journal_manager.read_page(page_id)? {
            Some(page) => {
                // find in journal, insert to cache
                self.page_cache.insert_to_cache(&page);

                return Ok(page);
            }

            None => (),
        }

        let offset = (page_id as u64) * (self.page_size as u64);
        let mut result = RawPage::new(page_id, self.page_size);
        result.read_from_file(&mut self.file, offset)?;

        self.page_cache.insert_to_cache(&result);

        #[cfg(feature = "log")]
        eprintln!("read page from main file, id: {}", page_id);

        Ok(result)
    }

    #[inline]
    pub fn free_page(&mut self, pid: u32) -> DbResult<()> {
        self.free_pages(&[pid])
    }

    pub fn free_pages(&mut self, pages: &[u32]) -> DbResult<()> {
        #[cfg(feature = "log")]
        for pid in pages {
            eprintln!("free page, id: {}", *pid);
        }

        let mut first_page = self.pipeline_read_page(0)?;
        let free_list_pid = header_page_utils::get_free_list_page_id(&first_page);
        if free_list_pid != 0 {
            return Err(DbErr::NotImplement);
        }

        let current_size = header_page_utils::get_free_list_size(&first_page);
        if (current_size as usize) + pages.len() >= header_page_utils::HEADER_FREE_LIST_MAX_SIZE {
            return Err(DbErr::NotImplement)
        }

        header_page_utils::set_free_list_size(&mut first_page, current_size + (pages.len() as u32));
        let mut counter = 0;
        for pid in pages {
            header_page_utils::set_free_list_content(&mut first_page, current_size + counter, *pid);
            counter += 1;
        }

        self.pipeline_write_page(&first_page)
    }

    pub fn is_journal_full(&self) -> bool {
        self.journal_manager.len() >= 1000
    }

    pub fn checkpoint_journal(&mut self) -> DbResult<()> {
        self.journal_manager.checkpoint_journal(&mut self.file)
    }

    fn try_get_free_page_id(&mut self) -> DbResult<Option<u32>> {
        let mut first_page = self.get_first_page()?;

        let free_list_size = header_page_utils::get_free_list_size(&first_page);
        if free_list_size == 0 {
            return Ok(None);
        }

        let result = header_page_utils::get_free_list_content(&first_page, free_list_size - 1);
        header_page_utils::set_free_list_size(&mut first_page, free_list_size - 1);

        self.pipeline_write_page(&first_page)?;

        Ok(Some(result))
    }

    #[inline]
    pub fn get_first_page(&mut self) -> Result<RawPage, DbErr> {
        self.pipeline_read_page(0)
    }

    pub fn alloc_page_id(&mut self) -> DbResult<u32> {
        match self.try_get_free_page_id()? {
            Some(page_id) =>  {

                #[cfg(feature = "log")]
                eprintln!("get new page_id from free list: {}", page_id);

                Ok(page_id)
            }

            None =>  {
                self.actual_alloc_page_id()
            }
        }
    }

    fn actual_alloc_page_id(&mut self) -> DbResult<u32> {
        let mut first_page = self.get_first_page()?;

        let null_page_bar = header_page_utils::get_null_page_bar(&first_page);
        header_page_utils::set_null_page_bar(&mut first_page, null_page_bar + 1);

        if (null_page_bar as u64) >= self.last_commit_db_size {  // truncate file
            let expected_size = self.last_commit_db_size + (DB_INIT_BLOCK_COUNT * self.page_size) as u64;

            self.last_commit_db_size = expected_size;
        }

        self.pipeline_write_page(&first_page)?;

        #[cfg(feature = "log")]
        eprintln!("alloc new page_id : {}", null_page_bar);

        Ok(null_page_bar)
    }

    #[inline]
    pub fn start_transaction(&mut self) -> DbResult<()> {
        self.journal_manager.start_transaction()
    }

    #[inline]
    pub fn commit(&mut self) -> DbResult<()> {
        self.journal_manager.commit()
    }

    #[inline]
    pub fn rollback(&mut self) -> DbResult<()> {
        self.journal_manager.rollback()
    }

}

