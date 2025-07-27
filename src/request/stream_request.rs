use crate::request::Request;
use crate::response::{ConsumeBodyConfig, Response};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

const DEFAULT_INITIAL_READ_SIZE: usize = 65536;

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
        match slf.as_super().consume_body_config() {
            ConsumeBodyConfig::Partially(size) => Ok(*size),
            ConsumeBodyConfig::Fully => Err(PyRuntimeError::new_err("Unexpected config")),
        }
    }

    #[setter]
    fn set_initial_read_size(mut slf: PyRefMut<Self>, init_size: usize) -> PyResult<()> {
        match slf.as_super().consume_body_config_mut() {
            ConsumeBodyConfig::Partially(size) => {
                *size = init_size;
                Ok(())
            }
            ConsumeBodyConfig::Fully => Err(PyRuntimeError::new_err("Unexpected config")),
        }
    }

    fn __copy__(slf: PyRefMut<Self>, py: Python) -> PyResult<Py<Self>> {
        Self::new_py(slf.into_super().try_clone_inner(py)?)
    }

    #[staticmethod]
    pub fn default_initial_read_size() -> usize {
        DEFAULT_INITIAL_READ_SIZE
    }
}
impl StreamRequest {
    pub fn new_py(inner: Request) -> PyResult<Py<Self>> {
        let initializer = PyClassInitializer::from(inner).add_subclass(Self { ctx_response: None });
        Python::with_gil(|py| Py::new(py, initializer))
    }
}
