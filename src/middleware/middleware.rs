use crate::asyncio::py_coro_waiter;
use crate::client::Client;
use crate::client::client::TaskLocal;
use crate::request::Request;
use crate::response::{Response, ResponseBuilder};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

#[pyclass(frozen)]
pub struct Next {
    client: Client,
    task_local: TaskLocal,
    current: usize,
}
#[pymethods]
impl Next {
    pub async fn run(
        slf: Py<Self>,
        request: Py<PyAny>,
        #[pyo3(cancel_handle)] cancel: CancelHandle,
    ) -> PyResult<Py<Response>> {
        let slf = slf.get();
        let middleware = slf.client.get_middleware(slf.current);

        if let Some(middleware) = middleware {
            slf.call_handle_inner(middleware, request, false).await
        } else {
            Request::send_inner(request, false, cancel).await // No more middleware, execute the request
        }
    }

    fn override_response_builder(&self) -> ResponseBuilder {
        ResponseBuilder::new(self.client.clone())
    }
}
impl Next {
    pub fn new(client: Client, task_local: TaskLocal) -> Self {
        Next {
            client,
            task_local,
            current: 0,
        }
    }

    pub async fn call_first(slf: Py<Self>, request: Py<PyAny>, error_for_status: bool) -> PyResult<Py<Response>> {
        let slf = slf.get();
        if slf.current != 0 {
            return Err(PyRuntimeError::new_err("Expected first middleware to be called"));
        }
        let Some(middleware) = slf.client.get_middleware(0) else {
            return Err(PyRuntimeError::new_err("Expected first middleware to be present"));
        };
        slf.call_handle_inner(middleware, request, error_for_status).await
    }

    async fn call_handle_inner(
        &self,
        middleware: &Py<PyAny>,
        request: Py<PyAny>,
        error_for_status: bool,
    ) -> PyResult<Py<Response>> {
        let fut = Python::with_gil(|py| {
            let next = Next {
                client: self.client.clone(),
                task_local: self.task_local.clone_ref(py),
                current: self.current + 1,
            };
            let coro = middleware.bind(py).call1((self.client.clone(), request, next))?;
            py_coro_waiter(coro, &self.task_local)
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
}
