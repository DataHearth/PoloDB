use std::io;
use std::fmt;
use std::num;
use crate::bson::value::Value;

#[derive(Debug)]
pub enum DbErr {
    ParseError,
    ParseIntError(num::ParseIntError),
    IOErr(io::Error),
    TypeMismatch(String, String),
    NotImplement,
    DecodeEOF,
    DecodeIntUnknownByte,
    DataOverflow,
    DataExist(Value),
    PageSpaceNotEnough,
    DataHasNoPrimaryKey,
}

impl fmt::Display for DbErr {

    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DbErr::ParseError => write!(f, "ParseError"),
            DbErr::ParseIntError(parse_int_err) => std::fmt::Display::fmt(&parse_int_err, f),
            DbErr::IOErr(io_err) => std::fmt::Display::fmt(&io_err, f),
            DbErr::TypeMismatch(expected, actual) =>
                write!(f, "TypeMismatch(expected: {}, actual: {})", expected, actual),
            DbErr::NotImplement => write!(f, "NotImplement"),
            DbErr::DecodeEOF => write!(f, "DecodeEOF"),
            DbErr::DecodeIntUnknownByte => write!(f, "DecodeIntUnknownByte"),
            DbErr::DataOverflow => write!(f, "DataOverflow"),
            DbErr::DataExist(value) => write!(f, "DataExist(pkey = {})", value.to_string()),
            DbErr::PageSpaceNotEnough => write!(f, "PageSpaceNotEnough"),
            DbErr::DataHasNoPrimaryKey => write!(f, "DataHasNoPrimaryKey"),
        }
    }

}

impl From<io::Error> for DbErr {

    fn from(error: io::Error) -> Self {
        DbErr::IOErr(error)
    }

}

impl From<num::ParseIntError> for DbErr {

    fn from(error: num::ParseIntError) -> Self {
        DbErr::ParseIntError(error)
    }

}
