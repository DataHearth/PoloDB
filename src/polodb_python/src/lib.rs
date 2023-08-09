use database::Database;
use pyo3::{pymodule, types::PyModule, PyResult, Python};

mod collection;
mod database;
mod utils;

/// A Python module implemented in Rust.
#[pymodule]
fn polodb_python(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Database>()?;
    Ok(())
}
