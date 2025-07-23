use crate::asyncio::{EventLoopCell, py_coro_waiter};
use crate::request::Request;
use crate::response::Response;
use pyo3::intern;
use pyo3::prelude::*;
use std::sync::Arc;

#[pyclass]
pub struct Next {
    middlewares: Arc<Vec<Py<PyAny>>>,
    current: usize,
    event_loop: Py<PyAny>,
}
#[pymethods]
impl Next {
    pub async fn run(slf: Py<Self>, request: Py<Request>) -> PyResult<Py<Response>> {
        if Next::is_last(&slf)? {
            let resp = Request::execute(&request).await?;
            Python::with_gil(|py| Py::new(py, resp))
        } else {
            Next::call_handle(slf, &request).await
        }
    }
}
impl Next {
    pub fn py_new(py: Python, middlewares: Arc<Vec<Py<PyAny>>>, event_loop: &mut EventLoopCell) -> PyResult<Py<Next>> {
        let next = Next {
            middlewares,
            current: 0,
            event_loop: event_loop.get_running_loop(py)?.clone_ref(py),
        };
        Python::with_gil(|py| Py::new(py, next))
    }

    pub async fn call_handle(slf: Py<Self>, request: &Py<Request>) -> PyResult<Py<Response>> {
        let fut = Python::with_gil(|py| {
            let this = slf.try_borrow(py)?;
            let middleware = &this.middlewares[this.current];
            let next = Next {
                middlewares: this.middlewares.clone(),
                current: this.current + 1,
                event_loop: this.event_loop.clone_ref(py),
            };

            let coro = middleware
                .bind(py)
                .call_method1(intern!(py, "handle"), (request, next))?;
            py_coro_waiter(&coro, this.event_loop.bind(py))
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
