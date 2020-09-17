/*
 * Copyright (c) 2020 Vincent Chan
 *
 * This program is free software; you can redistribute it and/or modify it under
 * the terms of the GNU Lesser General Public License as published by the Free Software
 * Foundation; either version 3, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE.  See the GNU Lesser General Public License for more
 * details.
 *
 * You should have received a copy of the GNU Lesser General Public License along with
 * this program.  If not, see <http://www.gnu.org/licenses/>.
 */
use std::rc::Rc;
use super::error::DbErr;
use super::page::{header_page_wrapper, PageHandler};
use crate::bson::ObjectIdMaker;
use crate::bson::{ObjectId, Document, Value, mk_str, mk_object_id};
use crate::btree::BTreePageInsertWrapper;
use crate::cursor::Cursor;

// root_btree schema
// {
//   _id: ObjectId,
//   name: String,
//   root_pid: Int,
//   flags: Int,
// }
//
// flags indicates:
// key_ty: 1byte
// ...
//

pub(crate) mod meta_document_key {
    pub(crate) static ID: &str       = "_id";
    pub(crate) static ROOT_PID: &str = "root_pid";
    pub(crate) static NAME: &str     = "name";
    pub(crate) static FLAGS: &str    = "flags";
    pub(crate) static INDEXES: &str  = "indexes";

    pub(crate) mod index {
        pub(crate) static NAME: &str = "name";
        pub(crate) static V: &str    = "v";
        pub(crate) static UNIQUE: &str = "unique";
        pub(crate) static ROOT_PID: &str = "root_pid";

    }

}

// #[derive(Clone)]
pub struct Database {
    ctx: Box<DbContext>,
}

pub type DbResult<T> = Result<T, DbErr>;

pub(crate) struct DbContext {
    page_handler :        Box<PageHandler>,

    pub obj_id_maker: ObjectIdMaker,

}

impl DbContext {

    fn new(path: &str) -> DbResult<DbContext> {
        let page_size = 4096;

        let page_handler = PageHandler::new(path, page_size)?;

        let obj_id_maker = ObjectIdMaker::new();

        let ctx = DbContext {
            page_handler: Box::new(page_handler),

            // first_page,
            obj_id_maker,
        };
        Ok(ctx)
    }

    #[inline]
    fn get_meta_page_id(&mut self) -> DbResult<u32> {
        let head_page = self.page_handler.pipeline_read_page(0)?;
        let head_page_wrapper = header_page_wrapper::HeaderPageWrapper::from_raw_page(head_page);
        let result = head_page_wrapper.get_meta_page_id();

        if result == 0 {  // unexpected
            return Err(DbErr::MetaPageIdError);
        }

        Ok(result)
    }

    pub fn create_collection(&mut self, name: &str) -> DbResult<ObjectId> {
        let oid = self.obj_id_maker.mk_object_id();
        let mut doc = Document::new_without_id();
        doc.insert(meta_document_key::ID.into(), mk_object_id(&oid));

        doc.insert(meta_document_key::NAME.into(), mk_str(name));

        let root_pid = self.page_handler.alloc_page_id()?;
        doc.insert(meta_document_key::ROOT_PID.into(), Value::Int(root_pid as i64));

        doc.insert(meta_document_key::FLAGS.into(), Value::Int(0));

        let meta_page_id: u32 = self.get_meta_page_id()?;

        let mut btree_wrapper = BTreePageInsertWrapper::new(&mut self.page_handler, meta_page_id);

        let insert_result = btree_wrapper.insert_item(&doc, false)?;

        match insert_result.backward_item {
            Some(backward_item) => {
                let new_root_id = self.page_handler.alloc_page_id()?;

                let raw_page = backward_item.write_to_page(&mut self.page_handler, new_root_id, meta_page_id)?;

                // update head page
                {
                    let head_page = self.page_handler.pipeline_read_page(0)?;
                    let mut head_page_wrapper = header_page_wrapper::HeaderPageWrapper::from_raw_page(head_page);
                    head_page_wrapper.set_meta_page_id(new_root_id);
                    self.page_handler.pipeline_write_page(&head_page_wrapper.0)?;
                }

                self.page_handler.pipeline_write_page(&raw_page)?;

                Ok(oid)
            }

            None => Ok(oid)
        }
    }

    pub fn create_index(&mut self, col_name: &str, keys: &Document, options: Option<&Document>) -> DbResult<()> {
        for (key_name, value_of_key) in keys.iter() {
            if let Value::Int(1) = value_of_key {
                // nothing
            } else {
                return Err(DbErr::InvalidOrderOfIndex(key_name.into()));
            }

            let meta_page_id = self.get_meta_page_id()?;
            let mut cursor = Cursor::new(&mut self.page_handler, meta_page_id)?;

            cursor.create_index(col_name, key_name, options)?;
        }

        Ok(())
    }

    #[inline]
    fn fix_doc(&mut self, mut doc: Rc<Document>) -> Rc<Document> {
        let id = doc.get("_id");
        match id {
            Some(_) => doc,
            None => {
                let new_doc = Rc::make_mut(&mut doc);
                new_doc.insert("_id".into(), Value::ObjectId(Rc::new(self.obj_id_maker.mk_object_id())));
                doc
            }
        }
    }

    fn insert(&mut self, col_name: &str, doc: Rc<Document>) -> DbResult<Rc<Document>> {
        let meta_page_id = self.get_meta_page_id()?;
        let doc = self.fix_doc(doc);
        let mut cursor = Cursor::new(&mut self.page_handler, meta_page_id)?;

        cursor.insert(col_name, doc.clone())?;

        Ok(doc)
    }

    fn delete(&mut self, col_name: &str, key: &Value) -> DbResult<Option<Rc<Document>>> {
        let meta_page_id = self.get_meta_page_id()?;
        let mut cursor = Cursor::new(&mut self.page_handler, meta_page_id)?;

        cursor.delete(col_name, key)
    }

    fn get_collection_cursor(&mut self, col_name: &str) -> DbResult<Cursor> {
        let root_page_id: i64 = {
            let meta_page_id = self.get_meta_page_id()?;
            let mut cursor = Cursor::new(&mut self.page_handler, meta_page_id)?;

            let mut tmp: i64 = -1;

            while cursor.has_next() {
                let ticket = cursor.peek().unwrap();
                let doc = cursor.get_doc_from_ticket(&ticket)?;

                let doc_name = match doc.get(meta_document_key::NAME) {
                    Some(name) => name,
                    None => return Err(DbErr::CollectionNotFound(col_name.into()))
                };

                if let Value::String(str_content) = doc_name {
                    if str_content.as_ref() == col_name {
                        tmp = match doc.get(meta_document_key::ROOT_PID) {
                            Some(Value::Int(pid)) => *pid,
                            _ => -1,
                        };
                        break;
                    }
                }

                let _ = cursor.next()?;
            }

            if tmp < 0 {
                return Err(DbErr::CollectionNotFound(col_name.into()))
            }

            tmp
        };

        Ok(Cursor::new(&mut self.page_handler, root_page_id as u32)?)
    }

    pub fn query_all_meta(&mut self) -> DbResult<Vec<Rc<Document>>> {
        let meta_page_id = self.get_meta_page_id()?;

        let mut result = vec![];
        let mut cursor = Cursor::new(&mut self.page_handler, meta_page_id)?;

        while cursor.has_next() {
            let ticket = cursor.peek().unwrap();
            let doc = cursor.get_doc_from_ticket(&ticket)?;
            result.push(doc);

            let _ = cursor.next()?;
        }

        Ok(result)
    }

    #[inline]
    pub fn start_transaction(&mut self) -> DbResult<()> {
        self.page_handler.start_transaction()
    }

    #[inline]
    pub fn commit(&mut self) -> DbResult<()> {
        self.page_handler.commit()
    }

    #[inline]
    #[allow(dead_code)]
    pub fn rollback(&mut self) -> DbResult<()> {
        self.page_handler.rollback()
    }

}

impl Drop for DbContext {

    fn drop(&mut self) {
        let _ = self.page_handler.checkpoint_journal();  // ignored
    }

}

impl Database {

    pub fn open(path: &str) -> DbResult<Database>  {
        let ctx = DbContext::new(path)?;
        let rc_ctx = Box::new(ctx);

        Ok(Database {
            ctx: rc_ctx,
        })
    }

    #[inline]
    pub fn create_collection(&mut self, name: &str) -> DbResult<ObjectId> {
        self.ctx.start_transaction()?;
        let oid = self.ctx.create_collection(name)?;
        self.ctx.commit()?;
        Ok(oid)
    }

    #[inline]
    pub fn get_version(&self) -> String {
        const VERSION: &'static str = env!("CARGO_PKG_VERSION");
        return VERSION.into();
    }

    #[inline]
    pub fn insert(&mut self, col_name: &str, doc: Rc<Document>) -> DbResult<Rc<Document>> {
        self.ctx.start_transaction()?;
        let doc = self.ctx.insert(col_name, doc)?;
        self.ctx.commit()?;
        Ok(doc)
    }

    #[inline]
    pub fn delete(&mut self, col_name: &str, key: &Value) -> DbResult<Option<Rc<Document>>> {
        self.ctx.start_transaction()?;
        let result = self.ctx.delete(col_name, key)?;
        self.ctx.commit()?;
        Ok(result)
    }

    #[inline]
    pub fn create_index(&mut self, col_name: &str, keys: &Document, options: Option<&Document>) -> DbResult<()> {
        self.ctx.create_index(col_name, keys, options)
    }

    #[allow(dead_code)]
    pub(crate) fn query_all_meta(&mut self) -> DbResult<Vec<Rc<Document>>> {
        self.ctx.query_all_meta()
    }

}

#[cfg(test)]
mod tests {
    use crate::Database;
    use std::rc::Rc;
    use crate::bson::{Document, Value, mk_str};

    static TEST_SIZE: usize = 1000;

    fn prepare_db() -> Database {
        let _ = std::fs::remove_file("/tmp/test.db");
        let _ = std::fs::remove_file("/tmp/test.db.journal");

        Database::open("/tmp/test.db").unwrap()
    }

    fn create_and_return_db_with_items(size: usize) -> Database {
        let mut db = prepare_db();
        let _result = db.create_collection("test").unwrap();

        // let meta = db.query_all_meta().unwrap();

        for i in 0..size {
            let content = i.to_string();
            let mut new_doc = Document::new_without_id();
            new_doc.insert("content".into(), mk_str(&content));
            db.insert("test", Rc::new(new_doc)).unwrap();
        }

        db
    }

    #[test]
    fn test_create_collection() {
        let mut db = create_and_return_db_with_items(TEST_SIZE);

        let mut test_col_cursor = db.ctx.get_collection_cursor("test").unwrap();
        let mut counter = 0;
        while test_col_cursor.has_next() {
            // let ticket = test_col_cursor.peek().unwrap();
            // let doc = test_col_cursor.get_doc_from_ticket(&ticket).unwrap();
            let doc = test_col_cursor.next().unwrap().unwrap();
            println!("object: {}", doc);
            counter += 1;
        }

        assert_eq!(TEST_SIZE, counter)
    }

    #[test]
    fn test_insert_bigger_key() {
        let mut db = prepare_db();
        let _result = db.create_collection("test").unwrap();

        let mut doc = Document::new_without_id();

        let mut new_str: String = String::new();
        for _i in 0..32 {
            new_str.push('0');
        }

        doc.insert("_id".into(), Value::String(Rc::new(new_str.clone())));

        let _ = db.insert("test", Rc::new(doc)).unwrap();

        let mut cursor = db.ctx.get_collection_cursor("test").unwrap();

        let get_one = cursor.next().unwrap().unwrap();
        let get_one_id = get_one.get("_id").unwrap().unwrap_string();

        assert_eq!(get_one_id, new_str);
    }

    #[test]
    fn test_one_delete_item() {
        let mut db = prepare_db();
        let _ = db.create_collection("test").unwrap();

        let mut collection  = vec![];

        for i in 0..100 {
            let content = i.to_string();
            let mut new_doc = Document::new_without_id();
            new_doc.insert("content".into(), mk_str(&content));
            let ret_doc = db.insert("test", Rc::new(new_doc)).unwrap();
            collection.push(ret_doc);
        }

        let third = &collection[3];
        let third_key = third.get("_id").unwrap();
        assert!(db.delete("test", third_key).unwrap().is_some());
        assert!(db.delete("test", third_key).unwrap().is_none());
    }

    #[test]
    fn test_delete_all_item() {
        let mut db = prepare_db();
        let _ = db.create_collection("test").unwrap();

        let mut collection  = vec![];

        for i in 0..100 {
            let content = i.to_string();
            let mut new_doc = Document::new_without_id();
            new_doc.insert("content".into(), mk_str(&content));
            let ret_doc = db.insert("test", Rc::new(new_doc)).unwrap();
            collection.push(ret_doc);
        }

        for doc in &collection {
            let key = doc.get("_id").unwrap();
            db.delete("test", key).unwrap();
        }
    }

    #[test]
    fn print_value_size() {
        let size = std::mem::size_of::<crate::bson::Value>();
        assert_eq!(size, 16);
    }

}
