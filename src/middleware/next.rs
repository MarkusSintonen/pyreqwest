use crate::asyncio::{PyCoroWaiter, TaskLocal, py_coro_waiter};
use crate::request::Request;
use crate::response::Response;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::{PyTraverseError, PyVisit};
use std::sync::Arc;

#[pyclass]
pub struct Next {
    middlewares: Option<Arc<Vec<Py<PyAny>>>>,
    task_local: Option<TaskLocal>,
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
        if let Some(coro) = Python::with_gil(|py| Self::next_coro(slf.bind(py), &request))? {
            Self::coro_result(coro, false).await
        } else {
            Request::spawn_request(&request, cancel).await // No more middleware, execute the request
        }
    }

    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Some(middlewares) = &self.middlewares {
            for mw in middlewares.iter() {
                visit.call(mw)?;
            }
        }
        if let Some(middlewares) = &self.override_middlewares {
            for mw in middlewares.iter() {
                visit.call(mw)?;
            }
        }
        if let Some(task_local) = &self.task_local {
            task_local.__traverse__(visit)?;
        }
        Ok(())
    }

    pub fn __clear__(&mut self) {
        self.middlewares = None;
        self.override_middlewares = None;
        self.task_local = None;
    }
}
impl Next {
    pub fn new(middlewares: Arc<Vec<Py<PyAny>>>, task_local: TaskLocal) -> Self {
        Next {
            middlewares: Some(middlewares),
            task_local: Some(task_local),
            current: 0,
            override_middlewares: None,
        }
    }

    fn current_middleware(&self) -> Option<&Py<PyAny>> {
        if let Some(override_middlewares) = self.override_middlewares.as_ref() {
            override_middlewares.get(self.current)
        } else {
            self.middlewares.as_ref().unwrap().get(self.current)
        }
    }

    pub fn next_coro(slf: &Bound<Self>, request: &Py<PyAny>) -> PyResult<Option<PyCoroWaiter>> {
        let py = slf.py();
        let this = slf.try_borrow()?;

        let Some(middleware) = this.current_middleware() else {
            return Ok(None); // No more middlewares
        };

        let task_local = this
            .task_local
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Expected task_local"))?;

        let next = Next {
            task_local: Some(task_local.clone_ref(py)?),
            middlewares: this.middlewares.clone(),
            current: this.current + 1,
            override_middlewares: this
                .override_middlewares
                .as_ref()
                .map(|m| m.iter().map(|v| v.clone_ref(py)).collect()),
        };

        let coro = middleware.bind(py).call1((request, next))?;
        Ok(Some(py_coro_waiter(coro, task_local)?))
    }

    pub async fn coro_result(coro: PyCoroWaiter, error_for_status: bool) -> PyResult<Py<Response>> {
        let resp = coro.await?;

        Python::with_gil(|py| {
            let resp = resp.into_bound(py).downcast_into_exact::<Response>()?;
            error_for_status
                .then(|| resp.try_borrow()?.error_for_status())
                .transpose()?;
            Ok(resp.unbind())
        })
    }

    pub fn add_middleware(&mut self, middleware: Bound<PyAny>) -> PyResult<()> {
        let mut override_middlewares = Vec::new();
        if let Some(middlewares) = self.middlewares.as_ref() {
            override_middlewares.extend(middlewares.iter().map(|m| m.clone_ref(middleware.py())));
        }
        override_middlewares.push(middleware.unbind());
        self.override_middlewares = Some(override_middlewares);
        Ok(())
    }
}
