use futures_util::FutureExt;
use once_cell::sync::OnceCell;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::{Bound, Py, PyAny, PyResult, Python, pyclass, pymethods};
use std::pin::Pin;
use std::task::{Context, Poll};

fn get_running_loop(py: Python) -> PyResult<Bound<PyAny>> {
    static GET_EV_LOOP: GILOnceCell<Py<PyAny>> = GILOnceCell::new();
    GET_EV_LOOP.import(py, "asyncio", "get_running_loop")?.call0()
}

pub fn py_coro_waiter<'py>(py_coro: Bound<'py, PyAny>, event_loop: &Bound<'py, PyAny>) -> PyResult<PyCoroWaiter> {
    let (tx, rx) = tokio::sync::oneshot::channel();

    let task_creator = TaskCreator {
        event_loop: event_loop.as_unbound().clone_ref(py_coro.py()),
        callback: Py::new(py_coro.py(), TaskCallback { tx: Some(tx) })?,
        coro: py_coro.unbind(),
    };
    event_loop.call_method1("call_soon_threadsafe", (task_creator,))?;

    Ok(PyCoroWaiter { rx })
}

#[pyclass]
struct TaskCreator {
    event_loop: Py<PyAny>,
    callback: Py<TaskCallback>,
    coro: Py<PyAny>,
}
#[pymethods]
impl TaskCreator {
    fn __call__(&self, py: Python) -> PyResult<()> {
        let py_task = self.event_loop.bind(py).call_method1("create_task", (&self.coro,))?;
        py_task.call_method1("add_done_callback", (&self.callback,))?;
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
            Poll::Ready(Ok(res)) => Poll::Ready(res),
            Poll::Ready(Err(e)) => {
                Poll::Ready(Err(PyRuntimeError::new_err(format!("Failed to receive task result: {}", e))))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[pyclass]
struct TaskCallback {
    tx: Option<tokio::sync::oneshot::Sender<PyResult<Py<PyAny>>>>,
}
#[pymethods]
impl TaskCallback {
    fn __call__(&mut self, task: Bound<PyAny>) -> PyResult<()> {
        self.tx
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("tx already consumed"))?
            .send(self.task_result(task).map(|res| res.unbind()))
            .map_err(|_| PyRuntimeError::new_err("Failed to send task result"))
    }
}
impl TaskCallback {
    fn task_result<'py>(&self, task: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        match task.call_method0("exception") {
            Ok(task_exc) => {
                if task_exc.is_none() {
                    task.call_method0("result")
                } else {
                    Err(PyErr::from_value(task_exc))
                }
            }
            Err(err) => Err(err),
        }
    }
}

pub struct EventLoopCell {
    cell: OnceCell<Py<PyAny>>,
}
impl EventLoopCell {
    pub fn new() -> Self {
        EventLoopCell { cell: OnceCell::new() }
    }

    pub fn get_running_loop(&self, py: Python) -> PyResult<&Py<PyAny>> {
        self.cell.get_or_try_init(|| Ok(get_running_loop(py)?.unbind()))
    }

    pub fn clone(&self, py: Python) -> Self {
        let cell = match self.cell.get() {
            Some(value) => OnceCell::with_value(value.clone_ref(py)),
            None => OnceCell::new(),
        };
        EventLoopCell { cell }
    }
}
