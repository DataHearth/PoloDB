use std::path::PathBuf;

use pyo3::{exceptions, pyclass, pymethods, PyResult};

use crate::{collection::Collection, utils::map_polodb_err};

#[pyclass]
#[derive(Debug, Clone)]
pub enum DBType {
    Memory,
    File,
}

#[pyclass]
pub(crate) struct Database(pub polodb_core::Database);

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

    pub fn collection(&self, col_name: &str) -> PyResult<Collection> {
        Ok(Collection::new(self, col_name))
    }

    pub fn list_collections(&self) -> PyResult<Vec<String>> {
        Ok(self
            .0
            .list_collection_names()
            .map_err(map_polodb_err::<exceptions::PyRuntimeError>)?)
    }
}
