use crate::asyncio::py_coro_waiter;
use crate::middleware::{MiddlewareRequest, ResponseDict};
use crate::request::RequestWrapper;
use crate::response::Response;
use pyo3::intern;
use pyo3::prelude::*;
use std::sync::Arc;
use tokio::sync::OwnedSemaphorePermit;

#[pyclass]
pub struct Next {
    client: Arc<reqwest::Client>,
    middlewares: Arc<Vec<Py<PyAny>>>,
    current: usize,
    request_semaphore_permit: Option<OwnedSemaphorePermit>,
}
#[pymethods]
impl Next {
    async fn run(slf: Py<Self>, request: Py<MiddlewareRequest>) -> PyResult<Py<Response>> {
        if Next::is_last(&slf)? {
            let exec_fut = Python::with_gil(|py| {
                let mut this = slf.try_borrow_mut(py)?;
                let client = this.client.clone();
                let permit = this.request_semaphore_permit.take();
                Ok::<_, PyErr>(MiddlewareRequest::execute(request, client, permit))
            })?;

            let resp = exec_fut.await?;

            Python::with_gil(|py| Py::new(py, resp))
        } else {
            let handle_fut = Python::with_gil(|py| {
                let mut this = slf.try_borrow_mut(py)?;
                let next = Next {
                    client: this.client.clone(),
                    middlewares: this.middlewares.clone(),
                    current: this.current + 1,
                    request_semaphore_permit: this.request_semaphore_permit.take(),
                };

                let middleware = this.middlewares[this.current].bind(py);
                let coro = middleware.call_method1(intern!(py, "handle"), (request, next))?;
                py_coro_waiter(py, coro)
            })?;

            let resp = handle_fut.await?;

            Python::with_gil(|py| Ok::<_, PyErr>(resp.into_bound(py).downcast_into_exact::<Response>()?.unbind()))
        }
    }

    #[pyo3(signature = (**kwds))]
    async fn create_response_override<'py>(slf: Py<Self>, kwds: Option<ResponseDict>) -> PyResult<Py<Response>> {
        let resp: reqwest::Response = kwds.unwrap_or_default().try_into()?;

        let fut = Python::with_gil(|py| {
            let mut this = slf.try_borrow_mut(py)?;
            let permit = this.request_semaphore_permit.take();
            Ok::<_, PyErr>(Response::initialize(resp, permit))
        })?;

        let resp = fut.await?;

        Python::with_gil(|py| Py::new(py, resp))
    }
}
impl Next {
    pub async fn process(
        client: Arc<reqwest::Client>,
        middlewares: Arc<Vec<Py<PyAny>>>,
        request: RequestWrapper,
        request_semaphore_permit: Option<OwnedSemaphorePermit>,
    ) -> PyResult<Py<Response>> {
        let (next, request) = Python::with_gil(|py| {
            let next = Next {
                client,
                middlewares,
                current: 0,
                request_semaphore_permit,
            };
            let request = MiddlewareRequest::new(request);
            Ok::<_, PyErr>((Py::new(py, next)?, Py::new(py, request)?))
        })?;

        Next::run(next, request).await
    }

    fn is_last(slf: &Py<Self>) -> PyResult<bool> {
        Python::with_gil(|py| {
            let this = slf.try_borrow(py)?;
            Ok(this.current == this.middlewares.len())
        })
    }
}
