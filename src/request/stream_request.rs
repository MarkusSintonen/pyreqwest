use crate::allow_threads::AllowThreads;
use crate::http::RequestBody;
use crate::request::Request;
use crate::response::{Response, SyncResponse};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyType;
use pyo3::{PyTraverseError, PyVisit};

#[pyclass(extends=Request)]
pub struct StreamRequest(Ctx<Response>);

#[pyclass(extends=Request)]
pub struct SyncStreamRequest(Ctx<SyncResponse>);

struct Ctx<T> {
    ctx_response: Option<Py<T>>,
}

#[pymethods]
impl StreamRequest {
    async fn __aenter__(slf: Py<Self>, #[pyo3(cancel_handle)] cancel: CancelHandle) -> PyResult<Py<Response>> {
        let response = AllowThreads(Request::send_inner(slf.as_any(), cancel)).await?;

        Python::attach(|py| -> PyResult<_> {
            slf.try_borrow_mut(py)?.0.ctx_response = Some(response.clone_ref(py));
            Ok(())
        })?;
        Ok(response)
    }

    async fn __aexit__(
        slf: Py<Self>,
        _exc_type: Py<PyAny>,
        _exc_val: Py<PyAny>,
        _traceback: Py<PyAny>,
    ) -> PyResult<()> {
        Python::attach(|py| {
            slf.try_borrow_mut(py)?
                .0
                .ctx_response
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Must be used as a context manager"))?
                .try_borrow(py)?
                .as_super()
                .inner_close();
            Ok(())
        })
    }

    fn __copy__(slf: PyRef<Self>, py: Python) -> PyResult<Py<Self>> {
        Self::new_py(py, slf.as_super().try_clone_inner(py)?)
    }

    #[classmethod]
    pub fn from_request_and_body(
        _cls: &Bound<'_, PyType>,
        py: Python,
        request: Bound<PyAny>,
        body: Option<Bound<RequestBody>>,
    ) -> PyResult<Py<Self>> {
        Self::new_py(py, Request::inner_from_request_and_body(request, body)?)
    }

    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        let Some(resp) = self.0.ctx_response.as_ref() else {
            return Ok(());
        };
        visit.call(resp)
    }

    fn __clear__(&mut self) {
        self.0.ctx_response.take();
    }
}
impl StreamRequest {
    pub fn new_py(py: Python, inner: Request) -> PyResult<Py<Self>> {
        let ctx = Ctx { ctx_response: None };
        let initializer = PyClassInitializer::from(inner).add_subclass(Self(ctx));
        Py::new(py, initializer)
    }
}

#[pymethods]
impl SyncStreamRequest {
    fn __enter__(slf: Bound<Self>) -> PyResult<Py<SyncResponse>> {
        let response = Request::blocking_send_inner(slf.as_unbound().as_any())?;

        slf.try_borrow_mut()?.0.ctx_response = Some(response.clone_ref(slf.py()));
        Ok(response)
    }

    fn __exit__(
        &mut self,
        _exc_type: Py<PyAny>,
        _exc_val: Py<PyAny>,
        _traceback: Py<PyAny>,
        py: Python,
    ) -> PyResult<()> {
        self.0
            .ctx_response
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Must be used as a context manager"))?
            .try_borrow(py)?
            .as_super()
            .inner_close();
        Ok(())
    }

    fn __copy__(slf: PyRef<Self>, py: Python) -> PyResult<Py<Self>> {
        Self::new_py(py, slf.as_super().try_clone_inner(py)?)
    }

    #[classmethod]
    pub fn from_request_and_body(
        _cls: &Bound<'_, PyType>,
        py: Python,
        request: Bound<PyAny>,
        body: Option<Bound<RequestBody>>,
    ) -> PyResult<Py<Self>> {
        Self::new_py(py, Request::inner_from_request_and_body(request, body)?)
    }

    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        let Some(resp) = self.0.ctx_response.as_ref() else {
            return Ok(());
        };
        visit.call(resp)
    }

    fn __clear__(&mut self) {
        self.0.ctx_response.take();
    }
}
impl SyncStreamRequest {
    pub fn new_py(py: Python, inner: Request) -> PyResult<Py<Self>> {
        let ctx = Ctx { ctx_response: None };
        let initializer = PyClassInitializer::from(inner).add_subclass(Self(ctx));
        Py::new(py, initializer)
    }
}
