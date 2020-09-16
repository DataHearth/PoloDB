use std::rc::Rc;
use std::cmp::Ordering;

use crate::db::DbResult;
use crate::vli;
use crate::page::{RawPage, PageType};
use crate::error::{DbErr, parse_error_reason};
use crate::bson::{Value, ObjectId, ty_int};
use crate::data_ticket::DataTicket;

pub const HEADER_SIZE: u32      = 64;

// | right_pid | key_ty_int | key content | ticket  |
// | 4 bytes   | 2 bytes    | 12 bytes    | 6 bytes |
pub const ITEM_SIZE: u32        = 24;

const BTREE_ENTRY_KEY_CONTENT_SIZE: usize = 12;

pub enum SearchKeyResult {
    Node(usize),
    Index(usize),
}

#[derive(Clone)]
pub struct BTreeNode {
    pub parent_pid:  u32,
    pub pid:         u32,
    pub content:     Vec<BTreeNodeDataItem>,
    pub indexes:     Vec<u32>,
}

impl BTreeNode {

    #[inline]
    pub(crate) fn is_leaf(&self) -> bool {
        self.indexes[0] == 0
    }

    // binary search the content
    // find the content or index
    pub(crate) fn search(&self, key: &Value) -> DbResult<SearchKeyResult> {
        let mut low: i32 = 0;
        let mut high: i32 = (self.content.len() - 1) as i32;

        while low <= high {
            let middle = (low + high) / 2;
            let target_key = &self.content[middle as usize].key;

            let cmp_result = key.value_cmp(target_key)?;

            match cmp_result {
                Ordering::Equal =>
                    return Ok(SearchKeyResult::Node(middle as usize)),

                Ordering::Less => {
                    high = middle - 1;
                }

                Ordering::Greater => {
                    low = middle + 1;
                }

            }
        }

        Ok(SearchKeyResult::Index(std::cmp::max(low, high) as usize))
    }

    pub fn clone_with_content(&self, new_index: usize, new_item: BTreeNodeDataItem) -> BTreeNode {
        let mut content: Vec<BTreeNodeDataItem> = Vec::with_capacity(self.content.capacity());

        for (index, item) in self.content.iter().enumerate() {
            if index == new_index {
                content.push(new_item.clone());
            } else {
                content.push(item.clone());
            }
        }

        BTreeNode {
            parent_pid: self.parent_pid,
            pid: self.pid,
            content,
            indexes: self.indexes.clone(),
        }
    }

    // Offset 0: magic(2 bytes)
    // Offset 2: items_len(2 bytes)
    // Offset 4: left_pid (4 bytes)
    // Offset 8: next_pid (4 bytes)
    pub(crate) fn from_raw(page: &RawPage, parent_pid: u32, item_size: u32) -> DbResult<BTreeNode> {
        #[cfg(debug_assertions)]
        if page.page_id == 0 {
            panic!("page id is zero, parent pid: {}", parent_pid);
        }

        let page_type = PageType::BTreeNode;
        let magic = page_type.to_magic();
        if page.data[0..2] != magic {
            if page.data[0..2] == [0, 0] {  // null page
                return Ok(BTreeNode {
                    pid: page.page_id,
                    parent_pid,
                    content: vec![],
                    indexes: vec![ 0 ],
                });
            }
            return Err(DbErr::ParseError(parse_error_reason::UNEXPECTED_HEADER_FOR_BTREE_PAGE.into()));
        }

        let first_left_pid = page.get_u32(4);
        let mut content = vec![];
        let mut indexes = vec![ first_left_pid ];

        let len = page.get_u16(2);

        if (len as u32) > item_size {  // data error
            return Err(DbErr::ItemSizeGreaterThenExpected);
        }

        for i in 0..len {
            let offset: u32 = HEADER_SIZE + (i as u32) * ITEM_SIZE;

            let right_pid = page.get_u32(offset);

            let node_data_item = BTreeNode::parse_node_data_item(&page, offset)?;

            content.push(node_data_item);
            indexes.push(right_pid);
        }

        Ok(BTreeNode {
            pid: page.page_id,
            parent_pid,
            content,
            indexes,
        })
    }

    fn parse_node_data_item(page: &RawPage, begin_offset: u32) -> DbResult<BTreeNodeDataItem> {
        let is_complex = page.get_u8(begin_offset + 4);
        if is_complex != 0 {
            unimplemented!()
        }

        let key_ty_int = page.get_u8(begin_offset + 4 + 1);  // use to parse data

        let key = match key_ty_int {
            0 => return Err(DbErr::ParseError(parse_error_reason::KEY_TY_SHOULD_NOT_BE_ZERO.into())),

            ty_int::OBJECT_ID => {
                let oid_bytes_begin = (begin_offset + 6) as usize;
                let oid_bytes = &page.data[oid_bytes_begin..(oid_bytes_begin + 12)];
                let oid = ObjectId::deserialize(oid_bytes)?;

                Value::ObjectId(Rc::new(oid))
            }

            ty_int::BOOLEAN => {
                let value_begin_offset = (begin_offset + 6) as usize;
                let value = page.data[value_begin_offset];

                let bl_value = if value == 0 {
                    false
                } else {
                    true
                };

                Value::Boolean(bl_value)
            }

            ty_int::INT => {
                let value_begin_offset = (begin_offset + 6) as usize;

                let int_value = unsafe {
                    let buffer_ptr = page.data.as_ptr();
                    let (result, _) = vli::decode_u64_raw(buffer_ptr.add(value_begin_offset))?;
                    result
                };

                Value::Int(int_value as i64)
            }

            _ => unimplemented!(),

        };

        let ticket_bytes = (begin_offset + 6 + 12) as usize;
        let ticket_bytes = &page.data[ticket_bytes..(ticket_bytes + 6)];
        let data_ticket = DataTicket::from_bytes(ticket_bytes);

        Ok(BTreeNodeDataItem {
            key,
            data_ticket,
        })
    }

    pub(crate) fn to_raw(&self, page: &mut RawPage) -> DbResult<()> {
        let items_len = self.content.len() as u16;

        let page_type = PageType::BTreeNode;
        let magic = page_type.to_magic();
        page.seek(0);
        page.put(&magic);

        page.seek(2);
        page.put_u16(items_len);

        self.content.first().map(|_first| {
            page.seek(4);

            let left_id = self.indexes.first().expect("get first left id failed");
            page.put_u32(*left_id);
        });

        let mut index = 0;
        while index < self.content.len() {
            let item = &self.content[index];
            let right_pid = self.indexes[index + 1];

            let offset: u32 = HEADER_SIZE + (index as u32) * ITEM_SIZE;

            page.seek(offset);

            // 4 bytes for right_pid
            page.put_u32(right_pid);

            // put entry key
            BTreeNode::entry_key_to_bytes(page, &item.key)?;

            // 6 bytes for ticket
            let ticket_bytes = item.data_ticket.to_bytes();
            page.put(&ticket_bytes);

            index += 1;
        }

        Ok(())
    }

    // | flag   | key_type | key_content |
    // | 1 byte | 1 byte   | 12 bytes    |
    //
    // if the length of key_content is greater than 12
    // then the flag should be 1, and the key_content should be zero
    fn entry_key_to_bytes(page: &mut RawPage, key: &Value) -> DbResult<()> {
        match key {
            Value::ObjectId(oid) => {
                BTreeNode::put_standard_content_key(page, key);

                // 12 bytes for key content
                let mut buffer = Vec::with_capacity(BTREE_ENTRY_KEY_CONTENT_SIZE);
                oid.serialize(&mut buffer)?;

                page.put(&buffer);

                Ok(())
            }

            Value::Boolean(bl) => {
                BTreeNode::put_standard_content_key(page, key);

                let mut buffer: [u8; BTREE_ENTRY_KEY_CONTENT_SIZE] = [0; BTREE_ENTRY_KEY_CONTENT_SIZE];
                buffer[0] = if *bl {
                    1
                } else {
                    0
                };

                page.put(&buffer);

                Ok(())
            }

            Value::Int(int) => {
                BTreeNode::put_standard_content_key(page, key);

                let mut buffer = Vec::with_capacity(BTREE_ENTRY_KEY_CONTENT_SIZE);
                vli::encode(&mut buffer, *int)?;

                buffer.resize(BTREE_ENTRY_KEY_CONTENT_SIZE, 0);

                page.put(&buffer);

                Ok(())
            }

            Value::String(str) => {
                let str_len = str.len();

                if str_len > BTREE_ENTRY_KEY_CONTENT_SIZE {
                    return BTreeNode::put_string_complex_key(page, key);
                }

                BTreeNode::put_standard_content_key(page, key);
                let mut buffer: [u8; BTREE_ENTRY_KEY_CONTENT_SIZE] = [0; BTREE_ENTRY_KEY_CONTENT_SIZE];

                buffer[0..str_len].copy_from_slice(str.as_bytes());

                page.put(&buffer);

                Ok(())
            }

            _ => return Err(DbErr::NotAValidKeyType(key.ty_name().into()))
        }
    }

    // | 1      | ty_int |
    // | 1 byte | 1 byte |
    fn put_string_complex_key(page: &mut RawPage, key: &Value) -> DbResult<()> {
        let key_ty_int = key.ty_int();

        page.put_u8(1);
        page.put_u8(key_ty_int);

        let buffer: [u8; 12] = [0; 12];
        page.put(&buffer);

        Ok(())
    }

    // | 0      | ty_int |
    // | 1 byte | 1 byte |
    fn put_standard_content_key(page: &mut RawPage, key: &Value) {
        let key_ty_int = key.ty_int();

        // 2 bytes for key ty_int
        page.put_u8(0);
        page.put_u8(key_ty_int);
    }

    #[allow(dead_code)]
    fn is_root(&self) -> bool {
        self.parent_pid == 0
    }

    pub(crate) fn shift_head(&mut self) -> (u32, BTreeNodeDataItem) {
        if self.content.is_empty() {
            panic!("btree content is empty, pid: {}", self.pid);
        }

        let first_index = self.indexes[0];
        let first_content: BTreeNodeDataItem = self.content[0].clone();

        self.indexes.remove(0);
        self.content.remove(0);

        (first_index, first_content)
    }

    pub(crate) fn shift_last(&mut self) -> (BTreeNodeDataItem, u32) {
        if self.content.is_empty() {
            panic!("btree content is empty, pid: {}", self.pid);
        }

        let last_index = self.indexes[self.indexes.len() - 1];
        let last_content = self.content[self.content.len() - 1].clone();

        self.indexes.remove(self.indexes.len() - 1);
        self.content.remove(self.content.len() - 1);

        (last_content, last_index)
    }

    #[inline]
    pub(crate) fn insert_head(&mut self, idx: u32, item: BTreeNodeDataItem) {
        self.indexes.insert(0, idx);
        self.content.insert(0, item);
    }

    #[inline]
    pub(crate) fn insert_back(&mut self, item: BTreeNodeDataItem, idx: u32) {
        self.indexes.push(idx);
        self.content.push(item);
    }

}

#[derive(Clone)]
pub struct BTreeNodeDataItem {
    pub(crate) key:          Value,
    pub(crate) data_ticket:  DataTicket,
}
