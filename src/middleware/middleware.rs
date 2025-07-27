use crate::asyncio::py_coro_waiter;
use crate::client::Client;
use crate::request::Request;
use crate::response::Response;
use pyo3::intern;
use pyo3::prelude::*;

#[pyclass]
pub struct Next {
    client: Client,
    current: usize,
}
#[pymethods]
impl Next {
    pub async fn run(slf: Py<Self>, request: Py<Request>) -> PyResult<Py<Response>> {
        if Next::is_last(&slf)? {
            let resp = Request::execute(&request).await?;
            Python::with_gil(|py| Py::new(py, resp))
        } else {
            Next::call_handle(slf, &request, false).await
        }
    }
}
impl Next {
    pub fn new(client: Client) -> Self {
        Next { client, current: 0 }
    }

    pub async fn call_handle(slf: Py<Self>, request: &Py<Request>, error_for_status: bool) -> PyResult<Py<Response>> {
        let fut = Python::with_gil(|py| {
            let this = slf.try_borrow(py)?;
            let middleware = this.client.get_middleware(this.current).unwrap();
            let next = Next {
                client: this.client.clone(),
                current: this.current + 1,
            };
            let event_loop = this.client.get_event_loop().get_running_loop(py)?;

            let coro = middleware
                .bind(py)
                .call_method1(intern!(py, "handle"), (request, next))?;
            py_coro_waiter(coro, event_loop.bind(py))
        })?;

        let resp = fut.await?;

        Python::with_gil(|py| {
            let resp = resp.into_bound(py).downcast_into_exact::<Response>()?;
            if error_for_status {
                resp.try_borrow()?.error_for_status()?;
            }
            Ok(resp.unbind())
        })
    }

    fn is_last(slf: &Py<Self>) -> PyResult<bool> {
        Python::with_gil(|py| {
            let this = slf.try_borrow(py)?;
            Ok(this.client.get_middleware(this.current).is_none())
        })
    }
}
