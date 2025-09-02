use crate::http::Body;
use crate::request::Request;
use crate::response::{BodyConsumeConfig, Response};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyType;
use pyo3::{PyTraverseError, PyVisit};

#[pyclass(extends=Request)]
pub struct StreamRequest {
    ctx_response: Option<Py<Response>>,
}

#[pymethods]
impl StreamRequest {
    async fn __aenter__(slf: Py<Self>, #[pyo3(cancel_handle)] cancel: CancelHandle) -> PyResult<Py<Response>> {
        let resp = Request::send_inner(slf.as_any(), cancel).await?;

        Python::with_gil(|py| {
            slf.try_borrow_mut(py)?.ctx_response = Some(resp.clone_ref(py));
            Ok(resp)
        })
    }

    async fn __aexit__(
        slf: Py<Self>,
        _exc_type: Py<PyAny>,
        _exc_val: Py<PyAny>,
        _traceback: Py<PyAny>,
    ) -> PyResult<()> {
        Python::with_gil(|py| {
            let ctx_response = &slf.try_borrow(py)?.ctx_response;
            ctx_response
                .as_ref()
                .ok_or_else(|| PyRuntimeError::new_err("Must be used as a context manager"))?
                .try_borrow_mut(py)?
                .inner_close();
            Ok(())
        })
    }

    #[getter]
    fn get_read_buffer_limit(slf: PyRef<Self>) -> PyResult<usize> {
        match slf.as_super().body_consume_config() {
            BodyConsumeConfig::Streamed(conf) => Ok(conf.read_buffer_limit),
            BodyConsumeConfig::FullyConsumed => Err(PyRuntimeError::new_err("Unexpected config")),
        }
    }

    fn __copy__(slf: PyRefMut<Self>, py: Python) -> PyResult<Py<Self>> {
        Self::new_py(slf.into_super().try_clone_inner(py)?)
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

    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.ctx_response)
    }

    fn __clear__(&mut self) {
        self.ctx_response = None;
    }
}
impl StreamRequest {
    pub fn new_py(inner: Request) -> PyResult<Py<Self>> {
        let initializer = PyClassInitializer::from(inner).add_subclass(Self { ctx_response: None });
        Python::with_gil(|py| Py::new(py, initializer))
    }
}
