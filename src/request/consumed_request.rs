use crate::request::Request;
use crate::response::Response;
use pyo3::coroutine::CancelHandle;
use pyo3::prelude::*;

#[pyclass(extends=Request)]
pub struct ConsumedRequest;

#[pymethods]
impl ConsumedRequest {
    pub async fn send(slf: Py<Self>, #[pyo3(cancel_handle)] cancel: CancelHandle) -> PyResult<Py<Response>> {
        Request::send_inner(slf.as_any(), cancel).await
    }
}
impl ConsumedRequest {
    pub fn new_py(inner: Request) -> PyResult<Py<Self>> {
        let initializer = PyClassInitializer::from(inner).add_subclass(Self {});
        Python::with_gil(|py| Py::new(py, initializer))
    }
}
