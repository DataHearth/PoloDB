use std::path::PathBuf;

use polodb_core::Database;
use pyo3::{exceptions, pyclass, pymethods, PyResult};

use crate::errors::map_polodb_err;

#[pyclass]
#[derive(Debug, Clone)]
pub enum DBType {
    Memory,
    File,
}

#[pyclass]
pub(crate) struct Database(polodb_core::Database);

#[pymethods]
impl Database {
    #[new]
    pub fn open(db_type: DBType, path: Option<PathBuf>) -> PyResult<Self> {
        let db = if let DBType::File = db_type {
            if path.is_none() {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "Path must be specified for file database",
                ));
            }

            polodb_core::Database::open_file(path.unwrap())
                .map_err(map_polodb_err::<exceptions::PyRuntimeError>)?
        } else {
            polodb_core::Database::open_memory()
                .map_err(map_polodb_err::<exceptions::PyRuntimeError>)?
        };

        Ok(Self(db))
    }
}
