use futures_util::FutureExt;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::PyDict;
use pyo3::{Bound, Py, PyAny, PyResult, PyTraverseError, PyVisit, Python, intern, pyclass, pymethods};
use std::pin::Pin;
use std::task::{Context, Poll};

pub fn get_running_loop(py: Python) -> PyResult<Bound<PyAny>> {
    static GET_EV_LOOP: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    GET_EV_LOOP.import(py, "asyncio", "get_running_loop")?.call0()
}

pub fn py_coro_waiter(py_coro: Bound<PyAny>, task_local: &TaskLocal) -> PyResult<PyCoroWaiter> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let py = py_coro.py();

    let event_loop = task_local.event_loop()?;

    let task_creator = TaskCreator {
        on_done_callback: Py::new(py, TaskDoneCallback { tx: Some(tx) })?,
        event_loop: Some(event_loop.clone_ref(py)),
        coro: Some(py_coro.unbind()),
    };

    let kwargs = PyDict::new(py);
    kwargs.set_item("context", &task_local.context)?;
    event_loop.call_method(py, intern!(py, "call_soon_threadsafe"), (task_creator,), Some(&kwargs))?;

    Ok(PyCoroWaiter { rx })
}

#[pyclass]
struct TaskCreator {
    on_done_callback: Py<TaskDoneCallback>,
    event_loop: Option<Py<PyAny>>,
    coro: Option<Py<PyAny>>,
}
#[pymethods]
impl TaskCreator {
    fn __call__(&self, py: Python) -> PyResult<()> {
        match self.create_task(py) {
            Ok(_) => Ok(()),
            Err(e) => self.on_done_callback.try_borrow_mut(py)?.tx_send(Err(e)),
        }
    }

    // :NOCOV_START
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.event_loop)?;
        visit.call(&self.coro)
    }

    fn __clear__(&mut self) {
        self.event_loop = None;
        self.coro = None;
    } // :NOCOV_END
}
impl TaskCreator {
    fn create_task(&self, py: Python) -> PyResult<()> {
        let py_task = self
            .event_loop
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Expected event_loop"))?
            .bind(py)
            .call_method1(intern!(py, "create_task"), (&self.coro,))?;
        py_task.call_method1(intern!(py, "add_done_callback"), (&self.on_done_callback,))?;
        Ok(())
    }
}

pub struct PyCoroWaiter {
    rx: tokio::sync::oneshot::Receiver<PyResult<Py<PyAny>>>,
}
impl Future for PyCoroWaiter {
    type Output = PyResult<Py<PyAny>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.get_mut().rx.poll_unpin(cx) {
            Poll::Ready(ready) => {
                let res = ready
                    .map_err(|e| PyRuntimeError::new_err(format!("Failed to receive task result: {}", e)))
                    .flatten();
                Poll::Ready(res)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[pyclass]
struct TaskDoneCallback {
    tx: Option<tokio::sync::oneshot::Sender<PyResult<Py<PyAny>>>>,
}
#[pymethods]
impl TaskDoneCallback {
    fn __call__(&mut self, task: Bound<PyAny>) -> PyResult<()> {
        self.tx_send(task.call_method0(intern!(task.py(), "result")).map(|res| res.unbind()))
    }
}
impl TaskDoneCallback {
    pub fn tx_send(&mut self, res: PyResult<Py<PyAny>>) -> PyResult<()> {
        self.tx
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("tx already consumed"))?
            .send(res)
            .map_err(|_| PyRuntimeError::new_err("Failed to send task result"))
    }
}

pub struct TaskLocal {
    event_loop: Option<Py<PyAny>>,
    context: Option<Py<PyAny>>,
}
impl TaskLocal {
    pub fn current(py: Python) -> PyResult<Self> {
        static ONCE_CTX_VARS: PyOnceLock<Py<PyAny>> = PyOnceLock::new();

        Ok(TaskLocal {
            event_loop: Some(get_running_loop(py)?.unbind()),
            context: Some(
                ONCE_CTX_VARS
                    .import(py, "contextvars", "copy_context")?
                    .call0()?
                    .unbind(),
            ),
        })
    }

    pub fn event_loop(&self) -> PyResult<&Py<PyAny>> {
        self.event_loop
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Expected event_loop"))
    }

    pub fn clone_ref(&self, py: Python) -> PyResult<Self> {
        Ok(TaskLocal {
            event_loop: Some(
                self.event_loop
                    .as_ref()
                    .ok_or_else(|| PyRuntimeError::new_err("Expected event_loop"))?
                    .clone_ref(py),
            ),
            context: Some(
                self.context
                    .as_ref()
                    .ok_or_else(|| PyRuntimeError::new_err("Expected context"))?
                    .clone_ref(py),
            ),
        })
    }

    // :NOCOV_START
    pub fn __traverse__(&self, visit: &PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.event_loop)?;
        visit.call(&self.context)?;
        Ok(())
    }

    pub fn __clear__(&mut self) {
        self.event_loop = None;
        self.context = None;
    } // :NOCOV_END
}
