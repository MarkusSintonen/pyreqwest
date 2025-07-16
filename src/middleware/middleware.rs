use crate::asyncio::py_coro_waiter;
use crate::request::Request;
use crate::response::Response;
use pyo3::intern;
use pyo3::prelude::*;
use std::sync::Arc;

#[pyclass]
pub struct Next {
    middlewares: Arc<Vec<Py<PyAny>>>,
    current: usize,
}
#[pymethods]
impl Next {
    pub async fn run(slf: Py<Self>, request: Py<Request>) -> PyResult<Py<Response>> {
        if Next::is_last(&slf)? {
            let resp = Request::execute(request).await?;
            Python::with_gil(|py| Py::new(py, resp))
        } else {
            Next::call_handle(slf, request).await
        }
    }
}
impl Next {
    pub async fn execute_all(middlewares: Arc<Vec<Py<PyAny>>>, request: Py<Request>) -> PyResult<Py<Response>> {
        let next = Next {
            middlewares,
            current: 0,
        };
        let next = Python::with_gil(|py| Py::new(py, next))?;
        Next::call_handle(next, request).await
    }

    pub async fn call_handle(slf: Py<Self>, request: Py<Request>) -> PyResult<Py<Response>> {
        let fut = Python::with_gil(|py| {
            let this = slf.try_borrow(py)?;
            let middleware = &this.middlewares[this.current];
            let next = Next {
                middlewares: this.middlewares.clone(),
                current: this.current + 1,
            };

            let coro = middleware
                .bind(py)
                .call_method1(intern!(py, "handle"), (request, next))?;
            py_coro_waiter(py, coro)
        })?;

        let resp = fut.await?;

        Python::with_gil(|py| Ok::<_, PyErr>(resp.into_bound(py).downcast_into_exact::<Response>()?.unbind()))
    }

    fn is_last(slf: &Py<Self>) -> PyResult<bool> {
        Python::with_gil(|py| {
            let this = slf.try_borrow(py)?;
            Ok(this.current == this.middlewares.len())
        })
    }
}
