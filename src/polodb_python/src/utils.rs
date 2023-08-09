use std::collections::HashMap;

use pyo3::{exceptions, types::PyFloat, FromPyObject, PyErr, PyResult, PyTypeInfo};

pub(crate) fn map_polodb_err<E: PyTypeInfo>(err: polodb_core::Error) -> PyErr {
    PyErr::new::<exceptions::PyRuntimeError, _>(err.to_string())
}

pub enum Value {
    Bool(bool),
    Number(Number),
    String(String),
    List(Vec<Value>),
    Object(HashMap<String, Value>),
}

pub enum Number {
    Float32(f32),
    Float64(f64),
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    Int128(i128),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    UInt128(u128),
}

impl FromPyObject<'_> for Number {
    fn extract(ob: &'_ pyo3::PyAny) -> PyResult<Self> {
        if ob.is_instance_of::<pyo3::types::PyFloat>()? {
            let val = ob.cast_as::<PyFloat>()?.value();

            if val <= f32::MAX as f64 {
                Ok(Number::Float32(val as f32))
            } else {
                Ok(Number::Float64(val))
            }
        } else if ob.is_instance_of::<pyo3::types::PyLong>()? {
            let val = ob.cast_as::<pyo3::types::PyLong>()?.extract::<i128>()?;

            if val.is_positive() {
                let val = val as u128;

                if u8::try_from(val).is_ok() {
                    Ok(Number::UInt8(val as u8))
                } else if u16::try_from(val).is_ok() {
                    Ok(Number::UInt16(val as u16))
                } else if u32::try_from(val).is_ok() {
                    Ok(Number::UInt32(val as u32))
                } else if u64::try_from(val).is_ok() {
                    Ok(Number::UInt64(val as u64))
                } else {
                    Ok(Number::UInt128(val))
                }
            } else {
                if i8::try_from(val).is_ok() {
                    Ok(Number::Int8(val as i8))
                } else if i16::try_from(val).is_ok() {
                    Ok(Number::Int16(val as i16))
                } else if i32::try_from(val).is_ok() {
                    Ok(Number::Int32(val as i32))
                } else if i64::try_from(val).is_ok() {
                    Ok(Number::Int64(val as i64))
                } else {
                    Ok(Number::Int128(val))
                }
            }
        } else {
            Err(exceptions::PyTypeError::new_err(format!(
                "Invalid type: {}\n Only int(8,16,32,64,128), uint(8,16,32,64,128), float(32,64) are supported",
                ob.get_type().name()?
            )))
        }
    }
}

impl FromPyObject<'_> for Value {
    fn extract(ob: &'_ pyo3::PyAny) -> PyResult<Self> {
        if ob.is_instance_of::<pyo3::types::PyBool>()? {
            Ok(Value::Bool(ob.extract()?))
        } else if ob.is_instance_of::<pyo3::types::PyString>()? {
            Ok(Value::String(ob.extract()?))
        } else if ob.is_instance_of::<pyo3::types::PyFloat>()?
            || ob.is_instance_of::<pyo3::types::PyLong>()?
        {
            Ok(Value::Number(ob.extract()?))
        } else if ob.is_instance_of::<pyo3::types::PyList>()? {
            let mut list = Vec::new();

            for item in ob.cast_as::<pyo3::types::PyList>()?.iter() {
                list.push(item.extract()?);
            }

            Ok(Value::List(list))
        } else if ob.is_instance_of::<pyo3::types::PyDict>()? {
            let mut map = HashMap::new();

            for (key, value) in ob.cast_as::<pyo3::types::PyDict>()?.iter() {
                map.insert(key.extract()?, value.extract()?);
            }

            Ok(Value::Object(map))
        } else {
            Err(exceptions::PyTypeError::new_err(format!(
                "Unsupported type: {}",
                ob.get_type().name()?
            )))
        }
    }
}

// impl FromPyObject<'_> for HashMap<String, Value> {
//     fn extract(ob: &'_ pyo3::PyAny) -> PyResult<Self> {
//         todo!()
//     }
// }
