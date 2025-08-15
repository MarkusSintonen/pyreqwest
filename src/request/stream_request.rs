use crate::http::Body;
use crate::request::Request;
use crate::response::{BodyConsumeConfig, Response};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyType;

const DEFAULT_INITIAL_READ_SIZE: usize = 65536;

#[pyclass(extends=Request)]
pub struct StreamRequest {
    ctx_response: Option<Py<Response>>,
}

#[pymethods]
impl StreamRequest {
    async fn __aenter__(slf: Py<Self>, #[pyo3(cancel_handle)] cancel: CancelHandle) -> PyResult<Py<Response>> {
        let req = Python::with_gil(|py| slf.clone_ref(py));

        let resp = Request::send_inner(req.into_any(), cancel).await?;

        Python::with_gil(|py| {
            slf.try_borrow_mut(py)?.ctx_response = Some(resp.clone_ref(py));
            Ok(resp)
        })
    }

    async fn __aexit__(&mut self, _exc_type: Py<PyAny>, _exc_val: Py<PyAny>, _traceback: Py<PyAny>) -> PyResult<()> {
        let ctx_response = self
            .ctx_response
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Must be used as a context manager"))?;
        Python::with_gil(|py| {
            ctx_response.try_borrow_mut(py)?.inner_close();
            Ok(())
        })
    }

    #[getter]
    fn get_initial_read_size(slf: PyRef<Self>) -> PyResult<usize> {
        match slf.as_super().body_consume_config() {
            BodyConsumeConfig::Partially(size) => Ok(*size),
            BodyConsumeConfig::Fully => Err(PyRuntimeError::new_err("Unexpected config")),
        }
    }

    #[setter]
    fn set_initial_read_size(mut slf: PyRefMut<Self>, init_size: usize) -> PyResult<()> {
        match slf.as_super().body_consume_config_mut() {
            BodyConsumeConfig::Partially(size) => {
                *size = init_size;
                Ok(())
            }
            BodyConsumeConfig::Fully => Err(PyRuntimeError::new_err("Unexpected config")),
        }
    }

    fn __copy__(slf: PyRefMut<Self>, py: Python) -> PyResult<Py<Self>> {
        Self::new_py(slf.into_super().try_clone_inner(py)?)
    }

    #[staticmethod]
    pub fn default_initial_read_size() -> usize {
        DEFAULT_INITIAL_READ_SIZE
    }

    #[classmethod]
    pub fn from_request_and_body(
        _cls: &Bound<'_, PyType>,
        py: Python,
        request: Bound<PyAny>,
        body: Option<Bound<Body>>,
    ) -> PyResult<Py<Self>> {
        Self::new_py(Request::inner_from_request_and_body(py, request, body)?)
    }
}
impl StreamRequest {
    pub fn new_py(inner: Request) -> PyResult<Py<Self>> {
        let initializer = PyClassInitializer::from(inner).add_subclass(Self { ctx_response: None });
        Python::with_gil(|py| Py::new(py, initializer))
    }
}
