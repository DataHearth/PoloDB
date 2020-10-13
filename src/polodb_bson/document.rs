use std::rc::Rc;
use std::fmt;
use super::value::{Value, ty_int};
use super::linked_hash_map::{LinkedHashMap, Iter};
use crate::vli;
use crate::BsonResult;
use crate::error::{BsonErr, parse_error_reason};
use crate::array::Array;
use crate::object_id::{ ObjectIdMaker, ObjectId };

#[derive(Debug, Clone)]
pub struct Document {
    map: LinkedHashMap<String, Value>,
}

impl Document {

    pub fn new(id_maker: &mut ObjectIdMaker) -> Document {
        let id = id_maker.mk_object_id();
        let mut result = Document {
            map: LinkedHashMap::new(),
        };
        result.map.insert("_id".to_string(), Value::ObjectId(Rc::new(id)));
        result
    }

    pub fn new_without_id() -> Document {
        Document {
            map: LinkedHashMap::new(),
        }
    }

    #[inline]
    pub fn insert(&mut self, key: String, value: Value) -> Option<Value> {
        self.map.insert(key, value)
    }

    #[inline]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.map.get(key)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn pkey_id(&self) -> Option<Value> {
        self.map.get("_id".into()).map(|id| { id.clone() })
    }

    pub fn from_bytes(bytes: &[u8]) -> BsonResult<Document> {
        let mut doc = Document::new_without_id();

        unsafe {
            let mut ptr = bytes.as_ptr();
            while ptr.read() != 0 {
                let byte = ptr.read();
                ptr = ptr.add(1);

                match byte {
                    ty_int::NULL => {
                        let (key, to_ptr) = Document::parse_key(ptr)?;
                        ptr = to_ptr;

                        doc.map.insert(key, Value::Null);
                    }

                    ty_int::DOUBLE => {
                        let (key, to_ptr) = Document::parse_key(ptr)?;
                        ptr = to_ptr;

                        let mut buffer: [u8; 8] = [0; 8];
                        ptr.copy_to_nonoverlapping(buffer.as_mut_ptr(), 8);

                        let num = f64::from_be_bytes(buffer);
                        doc.map.insert(key, Value::Double(num));

                        ptr = ptr.add(8);
                    }

                    ty_int::BOOLEAN => {
                        let (key, to_ptr) = Document::parse_key(ptr)?;
                        ptr = to_ptr;

                        let bl_value = ptr.read();
                        ptr = ptr.add(1);

                        doc.map.insert(key, Value::Boolean(if bl_value != 0 {
                            true
                        } else {
                            false
                        }));
                    }

                    ty_int::INT => {
                        let (key, to_ptr) = Document::parse_key(ptr)?;
                        ptr = to_ptr;

                        let (integer, to_ptr) = vli::decode_u64_raw(ptr)?;
                        ptr = to_ptr;

                        doc.map.insert(key, Value::Int(integer as i64));
                    }

                    ty_int::STRING => {
                        let (key, to_ptr) = Document::parse_key(ptr)?;
                        ptr = to_ptr;

                        let (value, to_ptr) = Document::parse_key(ptr)?;
                        ptr = to_ptr;

                        doc.map.insert(key, Value::String(Rc::new(value)));
                    }

                    ty_int::OBJECT_ID => {
                        let (key, to_ptr) = Document::parse_key(ptr)?;
                        ptr = to_ptr;

                        let mut buffer: [u8; 12] = [0; 12];
                        ptr.copy_to_nonoverlapping(buffer.as_mut_ptr(), 12);

                        ptr = ptr.add(12);

                        let oid = ObjectId::deserialize(&buffer)?;

                        doc.map.insert(key, Value::ObjectId(Rc::new(oid)));
                    }

                    ty_int::ARRAY => {
                        let (key, to_ptr) = Document::parse_key(ptr)?;
                        ptr = to_ptr;

                        let (len, to_ptr) = vli::decode_u64_raw(ptr)?;
                        ptr = to_ptr;

                        let mut buffer = Vec::with_capacity(len as usize);
                        ptr.copy_to(buffer.as_mut_ptr(), len as usize);

                        ptr = ptr.add(len as usize);

                        let sub_arr = Array::from_bytes(&buffer)?;
                        doc.map.insert(key, Value::Array(Rc::new(sub_arr)));
                    }

                    ty_int::DOCUMENT => {
                        let (key, to_ptr) = Document::parse_key(ptr)?;
                        ptr = to_ptr;

                        let (len, to_ptr) = vli::decode_u64_raw(ptr)?;
                        ptr = to_ptr;

                        let mut buffer = Vec::with_capacity(len as usize);
                        ptr.copy_to(buffer.as_mut_ptr(), len as usize);

                        ptr = ptr.add(len as usize);

                        let sub_doc = Document::from_bytes(&buffer)?;

                        doc.map.insert(key, Value::Document(Rc::new(sub_doc)));
                    }

                    ty_int::BINARY => {
                        let (key, to_ptr) = Document::parse_key(ptr)?;
                        ptr = to_ptr;

                        let (len, to_ptr) = vli::decode_u64_raw(ptr)?;
                        ptr = to_ptr;

                        let mut buffer = Vec::with_capacity(len as usize);
                        ptr.copy_to(buffer.as_mut_ptr(), len as usize);

                        ptr = ptr.add(len as usize);

                        doc.map.insert(key, Value::Binary(Rc::new(buffer)));
                    }

                    _ => return Err(BsonErr::ParseError(parse_error_reason::UNEXPECTED_DOCUMENT_FLAG.into())),
                }

            }
        }

        Ok(doc)
    }

    pub unsafe fn parse_key(ptr: *const u8) -> BsonResult<(String, *const u8)> {
        let mut ptr = ptr;
        let mut buffer = Vec::with_capacity(128);
        while ptr.read() != 0 {
            buffer.push(ptr.read());
            ptr = ptr.add(1);
        }

        Ok((String::from_utf8_unchecked(buffer), ptr.add(1)))
    }

    fn value_to_bytes(key: &str, value: &Value, buffer: &mut Vec<u8>) -> BsonResult<()> {
        match value {
            Value::Null => {
                buffer.push(ty_int::NULL);

                Document::key_to_bytes(&key, buffer);
            }

            Value::Double(num) => {
                buffer.push(ty_int::DOUBLE);

                Document::key_to_bytes(&key, buffer);

                buffer.extend_from_slice(&num.to_be_bytes());
            }

            Value::Boolean(bl) => {
                buffer.push(ty_int::BOOLEAN);
                Document::key_to_bytes(&key, buffer);
                if *bl {
                    buffer.push(0x00);
                } else {
                    buffer.push(0x01);
                }
            }

            Value::Int(int_num) => {
                buffer.push(ty_int::INT);  // not standard, use vli
                Document::key_to_bytes(&key, buffer);
                vli::encode(buffer, *int_num).expect("encode vli error");
            }

            Value::String(str) => {
                buffer.push(ty_int::STRING);
                Document::key_to_bytes(&key, buffer);

                Document::key_to_bytes(&str, buffer);
            }

            Value::ObjectId(oid) => {
                buffer.push(ty_int::OBJECT_ID);
                Document::key_to_bytes(&key, buffer);

                oid.serialize(buffer)?;
            }

            Value::Array(arr) => {
                buffer.push(ty_int::ARRAY);  // not standard
                Document::key_to_bytes(&key, buffer);

                let tmp = arr.to_bytes()?;
                vli::encode( buffer, tmp.len() as i64)?;

                buffer.extend(&tmp);
            }

            Value::Document(doc) => {
                buffer.push(ty_int::DOCUMENT);
                Document::key_to_bytes(&key, buffer);

                let tmp = doc.to_bytes()?;
                vli::encode(buffer, tmp.len() as i64)?;

                buffer.extend(&tmp);
            }

            Value::Binary(bin) => {
                buffer.push(ty_int::BINARY);

                Document::key_to_bytes(&key, buffer);

                vli::encode(buffer, bin.len() as i64)?;

                buffer.extend_from_slice(bin);
            }
        }

        Ok(())
    }

    pub fn to_bytes(&self) -> BsonResult<Vec<u8>> {
        let mut result: Vec<u8> = vec![];

        // insert id first
        let mut is_id_inserted = false;

        if let Some(id_value) = self.map.get("_id") {
            Document::value_to_bytes("_id", id_value, &mut result)?;
            is_id_inserted = true;
        }

        for (key, value) in &self.map {
            if is_id_inserted && key == "_id" {
                continue;
            }

            Document::value_to_bytes(key, value, &mut result)?;
        }

        result.push(0);

        Ok(result)
    }

    #[inline]
    pub fn iter(&self) -> Iter<String, Value> {
        self.map.iter()
    }

    fn key_to_bytes(key: &str, data: &mut Vec<u8>) {
        data.extend_from_slice(key.as_bytes());
        data.push(0); // cstring end
    }

}

#[cfg(test)]
mod tests {
    use crate::document::Document;
    // use crate::object_id::ObjectIdMaker;

    #[test]
    fn test_serialize() {
        // let mut id_maker = ObjectIdMaker::new();

        let doc = mk_document! {
            "avater_utl": "https://doc.rust-lang.org/std/iter/trait.Iterator.html",
            "name": "嘻嘻哈哈",
            "group_id": "70xxx80057ba0bba964fxxx1ca3d7252fe075a8b",
            "user_id": "6500xxx139040719xxx",
            "time": 6662496067319235000_i64,
            "can_do_a": true,
            "can_do_b": false,
            "can_do_c": false,
            "permissions": mk_array![ 1, 2, 3 ],
        };

        let bytes = doc.to_bytes().expect("serial error");

        let parsed_doc = Document::from_bytes(&bytes).expect("deserialize error");

        assert_eq!(parsed_doc.len(), doc.len());
    }

}

impl fmt::Display for Document {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{ ")?;

        let mut index = 0;
        for (key, value) in &self.map {
            write!(f, "{}: {}", key, value)?;

            if index < self.map.len() - 1 {
                write!(f, ", ")?;
            }
            index += 1;
        }

        write!(f, " }}")
    }

}
