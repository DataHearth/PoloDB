use std::collections::HashMap;

use bson::Document;
use pyo3::{exceptions, pyclass, pymethods, types::PyDict, PyResult};

use crate::{
    database::Database,
    utils::{map_polodb_err, Value},
};

#[pyclass]
pub(crate) struct Collection(polodb_core::Collection<Document>);

#[pymethods]
impl Collection {
    #[new]
    pub fn new(db: &Database, col_name: &str) -> Self {
        Self(db.0.collection::<Document>(col_name))
    }

    pub fn name(&self) -> &str {
        self.0.name()
    }

    pub fn count_documents(&self) -> PyResult<u64> {
        Ok(self
            .0
            .count_documents()
            .map_err(map_polodb_err::<exceptions::PyRuntimeError>)?)
    }

    pub fn update_one(&self, query: &PyDict, update: &PyDict) -> PyResult<()> {
        let query = query.extract::<HashMap<String, Value>>()?;
        let res = self
            .0
            .update_one(
                bson::to_document(todo!()).unwrap(),
                bson::to_document(todo!()).unwrap(),
            )
            .map_err(map_polodb_err::<exceptions::PyRuntimeError>)?;

        todo!()
    }
}
