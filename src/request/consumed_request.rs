use crate::allow_threads::AllowThreads;
use crate::http::RequestBody;
use crate::request::Request;
use crate::response::{Response, SyncResponse};
use pyo3::coroutine::CancelHandle;
use pyo3::prelude::*;
use pyo3::types::PyType;

#[pyclass(extends=Request)]
pub struct ConsumedRequest;

#[pyclass(extends=Request)]
pub struct SyncConsumedRequest;

#[pymethods]
impl ConsumedRequest {
    pub async fn send(slf: Py<Self>, #[pyo3(cancel_handle)] cancel: CancelHandle) -> PyResult<Py<Response>> {
        AllowThreads(Request::send_inner(slf.as_any(), cancel)).await
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
}
impl ConsumedRequest {
    pub fn new_py(py: Python, inner: Request) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self))
    }
}

#[pymethods]
impl SyncConsumedRequest {
    pub fn send(slf: Py<Self>) -> PyResult<Py<SyncResponse>> {
        Request::blocking_send_inner(slf.as_any())
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
}
impl SyncConsumedRequest {
    pub fn new_py(py: Python, inner: Request) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self))
    }
}
