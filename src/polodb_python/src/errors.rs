use pyo3::{exceptions, PyErr, PyTypeInfo};

pub(crate) fn map_polodb_err<E: PyTypeInfo>(err: polodb_core::Error) -> PyErr {
    PyErr::new::<exceptions::PyRuntimeError, _>(err.to_string())
}
