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
use std::fmt;
use std::cmp::Ordering;
use super::ObjectId;
use super::document::Document;
use super::array::Array;
use super::hex;
use crate::db::DbResult;
use crate::error::DbErr;

const BINARY_MAX_DISPLAY_LEN: usize = 64;

#[inline]
pub fn mk_object_id(content: &ObjectId) -> Value {
    Value::ObjectId(Rc::new(content.clone()))
}

#[inline]
pub fn mk_str(content: &str) -> Value {
    Value::String(Rc::new(content.into()))
}

#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Double(f64),
    Boolean(bool),

    // memory represent should use i64,
    // compress int when store on disk
    Int(i64),

    String(Rc<String>),
    ObjectId(Rc<ObjectId>),
    Array(Rc<Array>),
    Document(Rc<Document>),

    Binary(Rc<Vec<u8>>),
}

impl Value {

    pub fn value_cmp(&self, other: &Value) -> DbResult<Ordering> {
        match (self, other) {
            (Value::Null, Value::Null) => Ok(Ordering::Equal),
            (Value::Int(i1), Value::Int(i2)) => Ok(i1.cmp(i2)),
            (Value::String(str1), Value::String(str2)) => Ok(str1.cmp(str2)),
            (Value::ObjectId(oid1), Value::ObjectId(oid2)) => Ok(oid1.cmp(oid2)),
            _ =>
                return Err(DbErr::TypeNotComparable(self.ty_name().into(), other.ty_name().into()))
        }
    }

    pub fn ty_name(&self) -> &str {
        match self {
            Value::Null        => "Null",
            Value::Double(_)   => "Double",
            Value::Boolean(_)  => "Boolean",
            Value::Int(_)      => "Int",
            Value::String(_)   => "String",
            Value::ObjectId(_) => "ObjectId",
            Value::Array(_)    => "Array",
            Value::Document(_) => "Document",
            Value::Binary(_)   => "Binary",
        }
    }

    pub fn ty_int(&self) -> u8 {
        match self {
            Value::Null        => ty_int::NULL,
            Value::Double(_)   => ty_int::DOUBLE,
            Value::Boolean(_)  => ty_int::BOOLEAN,
            Value::Int(_)      => ty_int::INT,
            Value::String(_)   => ty_int::STRING,
            Value::ObjectId(_) => ty_int::OBJECT_ID,
            Value::Array(_)    => ty_int::ARRAY,
            Value::Document(_) => ty_int::DOCUMENT,
            Value::Binary(_)   => ty_int::BINARY,

        }
    }

    #[inline]
    pub fn unwrap_document(&self) -> &Rc<Document> {
        match self {
            Value::Document(doc) => doc,
            _ => panic!("unwrap error: document expected, but it's {}", self.ty_name()),
        }
    }

    #[inline]
    pub fn unwrap_boolean(&self) -> bool {
        match self {
            Value::Boolean(bl) => *bl,
            _ => panic!("unwrap error: boolean expected, but it's {}", self.ty_name()),
        }
    }

    #[inline]
    pub fn unwrap_int(&self) -> i64 {
        match self {
            Value::Int(i) => *i,
            _ => panic!("unwrap error: int expected, but it's {}", self.ty_name()),
        }
    }

    #[inline]
    pub fn unwrap_string(&self) -> &str {
        match self {
            Value::String(str) => str,
            _ => panic!("unwrap error: string expected, but it's {}", self.ty_name()),
        }
    }

    pub fn is_valid_key_type(&self) -> bool {
        match self {
            Value::String(_) |
            Value::Int(_) |
            Value::ObjectId(_) |
            Value::Boolean(_) => true,

            _ => false

        }
    }

}

impl fmt::Display for Value {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "Null"),

            Value::Double(num) => write!(f, "Double({})", num),

            Value::Boolean(bl) => if *bl {
                write!(f, "true")
            } else {
                write!(f, "false")
            },

            Value::Int(num) => write!(f, "{}", num),

            Value::String(str) => write!(f, "\"{}\"", str),

            Value::ObjectId(oid) => write!(f, "ObjectId({})", oid),

            Value::Array(arr) => write!(f, "Array(len = {})", arr.len()),

            Value::Document(_) => write!(f, "Document(...)"),

            Value::Binary(bin) => {
                if bin.len() > BINARY_MAX_DISPLAY_LEN {
                    return write!(f, "Binary(...)");
                }

                let hex_string_content = hex::encode(bin.as_ref());
                write!(f, "Binary({})", hex_string_content)
            }

        }
    }

}

pub mod ty_int {
    pub const NULL: u8       = 0x0A;
    pub const DOUBLE: u8     = 0x01;
    pub const BOOLEAN: u8    = 0x08;
    pub const INT: u8        = 0x16;
    pub const STRING: u8     = 0x02;
    pub const OBJECT_ID: u8  = 0x07;
    pub const ARRAY: u8      = 0x17;
    pub const DOCUMENT: u8   = 0x13;
    pub const BINARY: u8     = 0x05;

    pub fn to_str(i: u8) -> &'static str {
        match i {
            NULL => "Null",
            BOOLEAN => "Boolean",
            INT => "Int",
            STRING => "String",
            OBJECT_ID => "ObjectId",
            ARRAY => "Array",
            DOCUMENT => "Document",
            BINARY => "Binary",

            _ => "<unknown>"
        }
    }

}
