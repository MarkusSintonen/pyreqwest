use crate::http::HeaderMap;
use crate::response::BaseResponseBodyReader;
use bytes::Bytes;
use pyo3::PyVisit;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_bytes::PyBytes;

#[pyclass(frozen)]
pub struct JsonLoadsContext {
    #[pyo3(get)]
    pub body_reader: Py<BaseResponseBodyReader>,
    #[pyo3(get)]
    pub headers: Py<HeaderMap>,
    #[pyo3(get)]
    pub extensions: Py<PyDict>,
}

#[pyclass(frozen)]
pub struct JsonDumpsContext {
    #[pyo3(get)]
    pub data: Py<PyAny>,
}

#[pyclass(frozen)]
#[derive(Default)]
pub struct JsonHandler {
    dumps: Option<Py<PyAny>>,
    loads: Option<Py<PyAny>>,
}
impl JsonHandler {
    pub fn set_loads(&mut self, loads: Option<Bound<PyAny>>) {
        self.loads = loads.map(|v| v.unbind());
    }

    pub fn set_dumps(&mut self, dumps: Option<Bound<PyAny>>) {
        self.dumps = dumps.map(|v| v.unbind());
    }

    pub fn has_loads(&self) -> bool {
        self.loads.is_some()
    }

    pub fn has_dumps(&self) -> bool {
        self.dumps.is_some()
    }

    pub fn call_loads<'py>(&self, py: Python<'py>, context: JsonLoadsContext) -> PyResult<Bound<'py, PyAny>> {
        self.loads
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Expected loads callback"))?
            .bind(py)
            .call1((context,))
    }

    pub fn call_dumps(&self, py: Python, context: JsonDumpsContext) -> PyResult<Bytes> {
        Ok(self
            .dumps
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Expected loads callback"))?
            .bind(py)
            .call1((context,))?
            .extract::<PyBytes>()?
            .into_inner())
    }

    pub fn clone_ref(&self, py: Python) -> Self {
        JsonHandler {
            loads: self.loads.as_ref().map(|v| v.clone_ref(py)),
            dumps: self.dumps.as_ref().map(|v| v.clone_ref(py)),
        }
    }

    // :NOCOV_START
    pub fn __traverse__(&self, visit: &PyVisit) -> Result<(), pyo3::PyTraverseError> {
        visit.call(&self.loads)?;
        visit.call(&self.dumps)
    }

    pub fn __clear__(&mut self) {
        self.loads = None;
        self.dumps = None;
    } // :NOCOV_END
}

#[pymethods]
impl JsonLoadsContext {
    // :NOCOV_START
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), pyo3::PyTraverseError> {
        visit.call(&self.body_reader)?;
        visit.call(&self.headers)?;
        visit.call(&self.extensions)
    } // :NOCOV_END
}

#[pymethods]
impl JsonDumpsContext {
    // :NOCOV_START
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), pyo3::PyTraverseError> {
        visit.call(&self.data)
    } // :NOCOV_END
}
