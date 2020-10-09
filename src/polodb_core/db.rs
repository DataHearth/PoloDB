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
use polodb_bson::{Document, ObjectId, Value};
use super::error::DbErr;
use crate::context::DbContext;
use crate::DbHandle;

/*
 * API wrapper for Rust-level
 */
pub struct Database {
    ctx: Box<DbContext>,
}

pub type DbResult<T> = Result<T, DbErr>;

impl Database {

    #[inline]
    pub fn mk_object_id(&mut self) -> ObjectId {
        self.ctx.object_id_maker().mk_object_id()
    }

    pub fn open(path: &str) -> DbResult<Database>  {
        let ctx = DbContext::new(path)?;
        let rc_ctx = Box::new(ctx);

        Ok(Database {
            ctx: rc_ctx,
        })
    }

    pub fn create_collection(&mut self, name: &str) -> DbResult<()> {
        self.ctx.start_transaction()?;
        self.ctx.create_collection(name)?;
        self.ctx.commit()
    }

    #[inline]
    pub fn get_version() -> String {
        DbContext::get_version()
    }

    pub fn find_all(&mut self, col_name: &str) -> DbResult<Vec<Rc<Document>>> {
        let mut handle = self.ctx.find_all(col_name)?;

        let mut result = Vec::new();

        Database::consume_handle_to_vec(&mut handle, &mut result)?;

        Ok(result)
    }

    pub fn find(&mut self, col_name: &str, query: &Document) -> DbResult<Vec<Rc<Document>>> {
        let mut handle = self.ctx.find(col_name, query)?;

        let mut result = Vec::new();

        Database::consume_handle_to_vec(&mut handle, &mut result)?;

        Ok(result)
    }

    fn consume_handle_to_vec(handle: &mut DbHandle, result: &mut Vec<Rc<Document>>) -> DbResult<()> {
        handle.step();
        if handle.has_error() {
            let err = handle.take_error();
            return Err(err.unwrap());
        }

        while handle.has_row() {
            let doc = handle.get().unwrap_document();
            result.push(doc.clone());

            handle.step();
            if handle.has_error() {
                let err = handle.take_error();
                return Err(err.unwrap());
            }
        }

        Ok(())
    }

    #[inline]
    pub fn update(&mut self, col_name: &str, query: &Document, update: &Document) -> DbResult<()> {
        self.ctx.update(col_name, query, update)
    }

    pub fn insert(&mut self, col_name: &str, doc: Rc<Document>) -> DbResult<Rc<Document>> {
        self.ctx.start_transaction()?;
        let doc = self.ctx.insert(col_name, doc)?;
        self.ctx.commit()?;
        Ok(doc)
    }

    pub fn delete(&mut self, col_name: &str, key: &Value) -> DbResult<Option<Rc<Document>>> {
        self.ctx.start_transaction()?;
        let result = self.ctx.delete(col_name, key)?;
        self.ctx.commit()?;
        Ok(result)
    }

    pub fn create_index(&mut self, col_name: &str, keys: &Document, options: Option<&Document>) -> DbResult<()> {
        self.ctx.start_transaction()?;
        self.ctx.create_index(col_name, keys, options)?;
        self.ctx.commit()
    }

    #[allow(dead_code)]
    pub(crate) fn query_all_meta(&mut self) -> DbResult<Vec<Rc<Document>>> {
        self.ctx.query_all_meta()
    }

}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use std::env;
    use polodb_bson::{Document, Value, mk_document};
    use crate::Database;

    static TEST_SIZE: usize = 1000;

    fn prepare_db(db_name: &str) -> Database {
        let mut db_path = env::temp_dir();
        let mut journal_path = env::temp_dir();

        let db_filename = String::from(db_name) + ".db";
        let journal_filename = String::from(db_name) + ".db.journal";

        db_path.push(db_filename);
        journal_path.push(journal_filename);

        let _ = std::fs::remove_file(db_path.as_path());
        let _ = std::fs::remove_file(journal_path);

        Database::open(db_path.as_path().to_str().unwrap()).unwrap()
    }

    fn create_and_return_db_with_items(db_name: &str, size: usize) -> Database {
        let mut db = prepare_db(db_name);
        let _result = db.create_collection("test").unwrap();

        // let meta = db.query_all_meta().unwrap();

        for i in 0..size {
            let content = i.to_string();
            let mut new_doc = Document::new_without_id();
            new_doc.insert("content".into(), Value::from(content.as_str()));
            db.insert("test", Rc::new(new_doc)).unwrap();
        }

        db
    }

    #[test]
    fn test_create_collection_and_find_all() {
        let mut db = create_and_return_db_with_items("test-collection", TEST_SIZE);

        let all = db.find_all("test").unwrap();

        for doc in &all {
            println!("object: {}", doc);
        }

        assert_eq!(TEST_SIZE, all.len())
    }

    #[test]
    fn test_reopen_db() {
        {
            let _db1 = create_and_return_db_with_items("test-reopen", 5);
        }

        {
            let mut db_path = env::temp_dir();
            db_path.push("test-reopen.db");
            let _db2 = Database::open(db_path.as_path().to_str().unwrap()).unwrap();
        }
    }

    #[test]
    fn test_pkey_type_check() {
        let mut db = create_and_return_db_with_items("test-type-check", TEST_SIZE);

        let doc = mk_document! {
            "_id": 10,
            "value": "something",
        };

        db.insert("test", Rc::new(doc)).expect_err("should not succuess");
    }

    #[test]
    fn test_insert_bigger_key() {
        let mut db = prepare_db("test-insert-bigger-key");
        let _result = db.create_collection("test").unwrap();

        let mut doc = Document::new_without_id();

        let mut new_str: String = String::new();
        for _i in 0..32 {
            new_str.push('0');
        }

        doc.insert("_id".into(), Value::String(Rc::new(new_str.clone())));

        let _ = db.insert("test", Rc::new(doc)).unwrap();

        // let cursor = db.ctx.get_collection_cursor("test").unwrap();

        // let get_one = cursor.next().unwrap().unwrap();
        // let get_one_id = get_one.get("_id").unwrap().unwrap_string();

        // assert_eq!(get_one_id, new_str);
    }

    #[test]
    fn test_create_index() {
        let mut db = prepare_db("test-create-index");
        let _result = db.create_collection("test").unwrap();

        let keys = mk_document! {
            "user_id": 1,
        };

        db.create_index("test", &keys, None).unwrap();

        for i in 0..10 {
            let str = Rc::new(i.to_string());
            let data = mk_document! {
                "name": str.clone(),
                "user_id": str.clone(),
            };
            db.insert("test", Rc::new(data)).unwrap();
        }

        let data = mk_document! {
            "name": "what",
            "user_id": 3,
        };
        db.insert("test", Rc::new(data)).expect_err("not comparable");
    }

    #[test]
    fn test_one_delete_item() {
        let mut db = prepare_db("test-delete-item");
        let _ = db.create_collection("test").unwrap();

        let mut collection  = vec![];

        for i in 0..100 {
            let content = i.to_string();

            let new_doc = mk_document! {
                "content": content,
            };

            let ret_doc = db.insert("test", Rc::new(new_doc)).unwrap();
            collection.push(ret_doc);
        }

        let third = &collection[3];
        let third_key = third.get("_id").unwrap();
        assert!(db.delete("test", third_key).unwrap().is_some());
        assert!(db.delete("test", third_key).unwrap().is_none());
    }

    #[test]
    fn test_delete_all_items() {
        let mut db = prepare_db("test-delete-all-items");
        let _ = db.create_collection("test").unwrap();

        let mut collection  = vec![];

        for i in 0..100 {
            let content = i.to_string();
            let new_doc = mk_document! {
                "content": content,
            };
            let ret_doc = db.insert("test", Rc::new(new_doc)).unwrap();
            collection.push(ret_doc);
        }

        for doc in &collection {
            let key = doc.get("_id").unwrap();
            db.delete("test", key).unwrap();
        }
    }

}
