/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::path::{Path, PathBuf};
use polodb_core::LsmKv;
use polodb_core::test_utils::mk_db_path;

#[test]
fn test_insert_db() {
    vec![
        LsmKv::open_file(mk_db_path("test-insert-kv").as_path()).unwrap(),
        LsmKv::open_memory().unwrap(),
    ].iter().for_each(|db| {
        db.put("Hello", "World").unwrap();
        db.put(vec![1u8], vec![2u8]).unwrap();

        let value = db.get_string("Hello").unwrap().unwrap();
        assert_eq!(value, "World");

        let value = db.get(&[1u8]).unwrap().unwrap();
        assert_eq!(value.as_slice(), &[2u8]);
    });
}

fn clean_path(path: &Path) {
    let str = path.to_str().unwrap().to_string();
    let str_wal = str + ".wal";

    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(PathBuf::from(str_wal));
}

#[test]
fn test_persist() {
    let db_path = mk_db_path("test-kv-persist");
    clean_path(db_path.as_path());
    {
        let db = LsmKv::open_file(db_path.as_path()).unwrap();
        db.put("Hello", "World").unwrap();
        db.put("name", "Vincent").unwrap();
    }

    {
        let db = LsmKv::open_file(db_path.as_path()).unwrap();
        let value = db.get_string("Hello").unwrap().unwrap();
        assert_eq!(value, "World");
    }
}

#[test]
fn test_order() {
    vec![
        LsmKv::open_file(mk_db_path("test-kv-order").as_path()).unwrap(),
        LsmKv::open_memory().unwrap(),
    ].iter().for_each(|db| {
        for i in 0..10 {
            db.put(vec![i as u8], vec![i as u8]).unwrap();
        }

        let cursor = db.open_cursor();
        cursor.seek(vec![5 as u8]).unwrap();
        assert_eq!(cursor.value().unwrap().unwrap()[0], 5);

        cursor.next().unwrap();
        assert_eq!(cursor.value().unwrap().unwrap()[0], 6);
    });
}

#[test]
fn test_delete() {
    vec![
        LsmKv::open_file(mk_db_path("test-kv-delete").as_path()).unwrap(),
        LsmKv::open_memory().unwrap(),
    ].iter().for_each(|db| {
        for i in 0..10 {
            db.put(vec![i as u8], vec![i as u8]).unwrap();
        }

        db.delete(&[5u8]).unwrap();

        let cursor = db.open_cursor();
        cursor.seek(vec![4 as u8]).unwrap();

        assert_eq!(cursor.value().unwrap().unwrap()[0], 4);

        cursor.next().unwrap();
        assert_eq!(cursor.value().unwrap().unwrap()[0], 6);
    });
}
