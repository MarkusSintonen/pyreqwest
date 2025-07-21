use crate::request::Request;
use crate::response::Response;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

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
}
impl StreamRequest {
    pub fn new_py(inner: Request) -> PyResult<Py<Self>> {
        let initializer = PyClassInitializer::from(inner).add_subclass(Self { ctx_response: None });
        Python::with_gil(|py| Py::new(py, initializer))
    }
}
