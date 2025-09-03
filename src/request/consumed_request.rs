use crate::allow_threads::AllowThreads;
use crate::http::Body;
use crate::request::Request;
use crate::response::Response;
use pyo3::coroutine::CancelHandle;
use pyo3::prelude::*;
use pyo3::types::PyType;

#[pyclass(extends=Request)]
pub struct ConsumedRequest;

#[pymethods]
impl ConsumedRequest {
    pub async fn send(slf: Py<Self>, #[pyo3(cancel_handle)] cancel: CancelHandle) -> PyResult<Py<Response>> {
        AllowThreads(Request::send_inner(slf.as_any(), cancel)).await
    }

    pub fn blocking_send(slf: Py<Self>, py: Python) -> PyResult<Py<Response>> {
        let rt = { slf.try_borrow(py)?.as_super().spawner.runtime.0.clone() };
        py.detach(|| rt.block_on(async move { Request::send_inner(slf.as_any(), CancelHandle::new()).await }))
    }

    fn __copy__(slf: PyRef<Self>, py: Python) -> PyResult<Py<Self>> {
        Self::new_py(py, slf.as_super().try_clone_inner(py)?)
    }

    #[classmethod]
    pub fn from_request_and_body(
        _cls: &Bound<'_, PyType>,
        py: Python,
        request: Bound<PyAny>,
        body: Option<Bound<Body>>,
    ) -> PyResult<Py<Self>> {
        Self::new_py(py, Request::inner_from_request_and_body(py, request, body)?)
    }
}
impl ConsumedRequest {
    pub fn new_py(py: Python, inner: Request) -> PyResult<Py<Self>> {
        let initializer = PyClassInitializer::from(inner).add_subclass(Self {});
        Py::new(py, initializer)
    }
}
