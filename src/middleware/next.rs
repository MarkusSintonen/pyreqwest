use crate::asyncio::py_coro_waiter;
use crate::client::{Client, TaskLocal};
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
    override_middlewares: Option<Vec<Py<PyAny>>>,
}
#[pymethods]
impl Next {
    pub async fn run(
        slf: Py<Self>,
        request: Py<PyAny>,
        #[pyo3(cancel_handle)] cancel: CancelHandle,
    ) -> PyResult<Py<Response>> {
        let slf = slf.get();
        if let Some(middleware) = slf.current_middleware() {
            slf.call_handle_inner(middleware, request, false).await
        } else {
            Request::spawn_request(request, cancel).await // No more middleware, execute the request
        }
    }

    fn response_builder(&self) -> ResponseBuilder {
        ResponseBuilder::new(self.client.clone())
    }
}
impl Next {
    pub fn new(client: Client, task_local: TaskLocal, override_middlewares: Option<Vec<Py<PyAny>>>) -> Self {
        Next {
            client,
            task_local,
            current: 0,
            override_middlewares,
        }
    }

    fn current_middleware(&self) -> Option<&Py<PyAny>> {
        if let Some(override_middlewares) = self.override_middlewares.as_ref() {
            override_middlewares.get(self.current)
        } else {
            self.client.middlewares().unwrap().get(self.current)
        }
    }

    pub async fn call_first(&self, request: Py<PyAny>, error_for_status: bool) -> PyResult<Py<Response>> {
        if self.current != 0 {
            return Err(PyRuntimeError::new_err("Expected first middleware to be called"));
        }
        let Some(middleware) = self.current_middleware() else {
            return Err(PyRuntimeError::new_err("Expected first middleware to be present"));
        };
        self.call_handle_inner(middleware, request, error_for_status).await
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
                override_middlewares: self
                    .override_middlewares
                    .as_ref()
                    .map(|m| m.iter().map(|v| v.clone_ref(py)).collect()),
            };
            let coro = middleware.bind(py).call1((self.client.clone(), request, next))?;
            py_coro_waiter(coro, &self.task_local)
        })?;

        let resp = fut.await?;

        Python::with_gil(|py| {
            let resp = resp.into_bound(py).downcast_into_exact::<Response>()?;
            error_for_status
                .then(|| resp.try_borrow()?.error_for_status())
                .transpose()?;
            Ok(resp.unbind())
        })
    }
}
