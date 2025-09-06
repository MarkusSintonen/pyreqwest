use crate::allow_threads::AllowThreads;
use crate::http::RequestBody;
use crate::request::Request;
use crate::response::{BlockingResponse, Response};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyType;
use pyo3::{PyTraverseError, PyVisit};

#[pyclass(extends=Request)]
pub struct StreamRequest {
    ctx_response: Option<Py<Response>>,
}

#[pyclass(extends=Request)]
pub struct BlockingStreamRequest {
    ctx_response: Option<Py<BlockingResponse>>,
}

#[pymethods]
impl StreamRequest {
    async fn __aenter__(slf: Py<Self>, #[pyo3(cancel_handle)] cancel: CancelHandle) -> PyResult<Py<Response>> {
        let response = AllowThreads(Request::send_inner(slf.as_any(), cancel)).await?;

        Python::attach(|py| {
            slf.try_borrow_mut(py)?.ctx_response = Some(response.clone_ref(py));
            Ok(response)
        })
    }

    async fn __aexit__(
        slf: Py<Self>,
        _exc_type: Py<PyAny>,
        _exc_val: Py<PyAny>,
        _traceback: Py<PyAny>,
    ) -> PyResult<()> {
        Python::attach(|py| {
            let ctx_response = &slf.try_borrow(py)?.ctx_response;
            ctx_response
                .as_ref()
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
        Self::new_py(py, Request::inner_from_request_and_body(py, request, body)?)
    }

    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.ctx_response)
    }

    fn __clear__(&mut self) {
        self.ctx_response = None;
    }
}
impl StreamRequest {
    pub fn new_py(py: Python, inner: Request) -> PyResult<Py<Self>> {
        let initializer = PyClassInitializer::from(inner).add_subclass(Self { ctx_response: None });
        Py::new(py, initializer)
    }
}

#[pymethods]
impl BlockingStreamRequest {
    fn __enter__(slf: Py<Self>, py: Python) -> PyResult<Py<BlockingResponse>> {
        let response = Request::blocking_send_inner(slf.as_any())?;

        slf.bind(py).try_borrow_mut()?.ctx_response = Some(response.clone_ref(py));
        Ok(response)
    }

    fn __exit__(&self, _exc_type: Py<PyAny>, _exc_val: Py<PyAny>, _traceback: Py<PyAny>, py: Python) -> PyResult<()> {
        self.ctx_response
            .as_ref()
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
        Self::new_py(py, Request::inner_from_request_and_body(py, request, body)?)
    }

    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.ctx_response)
    }

    fn __clear__(&mut self) {
        self.ctx_response = None;
    }
}
impl BlockingStreamRequest {
    pub fn new_py(py: Python, inner: Request) -> PyResult<Py<Self>> {
        let initializer = PyClassInitializer::from(inner).add_subclass(Self { ctx_response: None });
        Py::new(py, initializer)
    }
}
