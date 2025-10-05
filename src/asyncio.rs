use futures_util::FutureExt;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::asyncio::CancelledError;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::{MutexExt, PyOnceLock};
use pyo3::types::PyDict;
use pyo3::{Bound, Py, PyAny, PyResult, PyTraverseError, PyVisit, Python, intern, pyclass, pymethods};
use std::pin::Pin;
use std::sync::{Mutex, MutexGuard};
use std::task::{Context, Poll};

pub fn get_running_loop(py: Python) -> PyResult<Bound<PyAny>> {
    static GET_EV_LOOP: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    GET_EV_LOOP.import(py, "asyncio", "get_running_loop")?.call0()
}

pub fn py_coro_waiter(
    py_coro: Bound<PyAny>,
    task_local: &TaskLocal,
    cancel_handle: Option<CancelHandle>,
) -> PyResult<PyCoroWaiter> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let py = py_coro.py();

    let event_loop = task_local.event_loop()?;

    let task_creator =
        TaskCreator::new(Py::new(py, TaskDoneCallback::new(tx))?, event_loop.clone_ref(py), py_coro.unbind());
    let task_creator = Py::new(py, task_creator)?;

    let kwargs = PyDict::new(py);
    kwargs.set_item("context", &task_local.context)?;
    event_loop.call_method(py, intern!(py, "call_soon_threadsafe"), (task_creator.clone_ref(py),), Some(&kwargs))?;

    Ok(PyCoroWaiter::new(rx, task_creator, cancel_handle))
}

pub fn is_async_callable(obj: &Bound<PyAny>) -> PyResult<bool> {
    if iscoroutinefunction(obj)? {
        return Ok(true);
    }
    if obj.hasattr(intern!(obj.py(), "__call__"))? {
        return iscoroutinefunction(&obj.getattr(intern!(obj.py(), "__call__"))?);
    }
    Ok(false)
}

fn iscoroutinefunction(obj: &Bound<PyAny>) -> PyResult<bool> {
    static IS_CORO_FUNC: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    if !obj.is_callable() {
        return Err(PyValueError::new_err("Expected a callable"));
    }
    IS_CORO_FUNC
        .import(obj.py(), "inspect", "iscoroutinefunction")?
        .call1((obj,))?
        .extract()
}

#[pyclass(frozen)]
struct TaskCreator(Mutex<InnerTaskCreator>);
struct InnerTaskCreator {
    on_done_callback: Py<TaskDoneCallback>,
    event_loop: Option<Py<PyAny>>,
    coro: Option<Py<PyAny>>,
    task: Option<Py<PyAny>>,
}
#[pymethods]
impl TaskCreator {
    fn __call__(&self, py: Python) -> PyResult<()> {
        self.lock(py)?.create_task(py)
    }

    // :NOCOV_START
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        let Ok(slf) = self.0.try_lock() else {
            return Ok(());
        };
        visit.call(&slf.event_loop)?;
        visit.call(&slf.coro)
    }

    fn __clear__(&self) {
        let Ok(mut slf) = self.0.try_lock() else {
            return;
        };
        slf.event_loop = None;
        slf.coro = None;
    } // :NOCOV_END
}
impl TaskCreator {
    fn new(on_done_callback: Py<TaskDoneCallback>, event_loop: Py<PyAny>, coro: Py<PyAny>) -> Self {
        TaskCreator(Mutex::new(InnerTaskCreator {
            on_done_callback,
            event_loop: Some(event_loop),
            coro: Some(coro),
            task: None,
        }))
    }

    fn cancel(&self, py: Python) -> PyResult<()> {
        self.lock(py)?.cancel(py)
    }

    fn lock(&self, py: Python) -> PyResult<MutexGuard<'_, InnerTaskCreator>> {
        self.0
            .lock_py_attached(py)
            .map_err(|_| PyRuntimeError::new_err("TaskCreator mutex poisoned"))
    }
}
impl InnerTaskCreator {
    fn create_task(&mut self, py: Python) -> PyResult<()> {
        fn inner_create<'py>(slf: &mut InnerTaskCreator, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
            let Some(coro) = slf.coro.take() else {
                return Err(CancelledError::new_err("Task was cancelled"));
            };
            let task = slf
                .event_loop
                .as_ref()
                .ok_or_else(|| PyRuntimeError::new_err("Expected event_loop"))?
                .bind(py)
                .call_method1(intern!(py, "create_task"), (&coro,))?;
            task.call_method1(intern!(py, "add_done_callback"), (&slf.on_done_callback,))?;
            Ok(task)
        }

        match inner_create(self, py) {
            Ok(task) => self.task = Some(task.unbind()),
            Err(e) => self.on_done_callback.get().tx_send(py, Err(e))?,
        }
        Ok(())
    }

    fn cancel(&mut self, py: Python) -> PyResult<()> {
        self.coro = None;
        if let Some(task) = self.task.take() {
            task.bind(py).call_method0(intern!(py, "cancel"))?;
        }
        Ok(())
    }
}

pub struct PyCoroWaiter {
    rx: tokio::sync::oneshot::Receiver<PyResult<Py<PyAny>>>,
    task_creator: Py<TaskCreator>,
    cancel_handle: Option<CancelHandle>,
}
impl PyCoroWaiter {
    fn new(
        rx: tokio::sync::oneshot::Receiver<PyResult<Py<PyAny>>>,
        task_creator: Py<TaskCreator>,
        cancel_handle: Option<CancelHandle>,
    ) -> Self {
        PyCoroWaiter {
            rx,
            task_creator,
            cancel_handle,
        }
    }
}
impl Future for PyCoroWaiter {
    type Output = PyResult<Py<PyAny>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(cancel_handle) = self.cancel_handle.as_mut() {
            match cancel_handle.poll_cancelled(cx) {
                Poll::Ready(_) => {
                    // Cancel inner task and cancel the Future right away
                    return match Python::attach(|py| self.task_creator.get().cancel(py)) {
                        Ok(()) => Poll::Ready(Err(CancelledError::new_err("Task was cancelled"))),
                        Err(e) => Poll::Ready(Err(e)),
                    };
                }
                Poll::Pending => {}
            }
        }

        match self.rx.poll_unpin(cx) {
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

type OnceResultSender = Option<tokio::sync::oneshot::Sender<PyResult<Py<PyAny>>>>;

#[pyclass(frozen)]
struct TaskDoneCallback(Mutex<OnceResultSender>);
#[pymethods]
impl TaskDoneCallback {
    fn __call__(&self, py: Python, task: Bound<PyAny>) -> PyResult<()> {
        let task_res = task.call_method0(intern!(task.py(), "result"));
        self.tx_send(py, task_res)
    }
}
impl TaskDoneCallback {
    fn new(tx: tokio::sync::oneshot::Sender<PyResult<Py<PyAny>>>) -> Self {
        TaskDoneCallback(Mutex::new(Some(tx)))
    }

    fn tx_send(&self, py: Python, res: PyResult<Bound<PyAny>>) -> PyResult<()> {
        self.lock(py)?
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("tx already consumed"))?
            .send(res.map(|r| r.unbind()))
            .map_err(|_| PyRuntimeError::new_err("Failed to send task result"))
    }

    fn lock(&self, py: Python) -> PyResult<MutexGuard<'_, OnceResultSender>> {
        self.0
            .lock_py_attached(py)
            .map_err(|_| PyRuntimeError::new_err("TaskDoneCallback mutex poisoned"))
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
