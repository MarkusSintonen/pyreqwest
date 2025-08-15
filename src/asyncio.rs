use crate::client::TaskLocal;
use futures_util::FutureExt;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::PyDict;
use pyo3::{Bound, Py, PyAny, PyResult, Python, intern, pyclass, pymethods};
use std::pin::Pin;
use std::task::{Context, Poll};

pub fn get_running_loop(py: Python) -> PyResult<Bound<PyAny>> {
    static GET_EV_LOOP: GILOnceCell<Py<PyAny>> = GILOnceCell::new();
    GET_EV_LOOP.import(py, "asyncio", "get_running_loop")?.call0()
}

pub fn py_coro_waiter(py_coro: Bound<PyAny>, task_local: &TaskLocal) -> PyResult<PyCoroWaiter> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let py = py_coro.py();

    let task_creator = TaskCreator {
        event_loop: task_local.event_loop.clone_ref(py),
        callback: Py::new(py, TaskCallback { tx: Some(tx) })?,
        coro: py_coro.unbind(),
    };

    let kwargs = PyDict::new(py);
    kwargs.set_item("context", &task_local.context)?;
    task_local
        .event_loop
        .call_method(py, intern!(py, "call_soon_threadsafe"), (task_creator,), Some(&kwargs))?;

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
        match self.create_task(py) {
            Ok(_) => Ok(()),
            Err(e) => self.callback.try_borrow_mut(py)?.tx_send(Err(e)),
        }
    }
}
impl TaskCreator {
    fn create_task(&self, py: Python) -> PyResult<()> {
        let py_task = self
            .event_loop
            .bind(py)
            .call_method1(intern!(py, "create_task"), (&self.coro,))?;
        py_task.call_method1(intern!(py, "add_done_callback"), (&self.callback,))?;
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
        self.tx_send(self.get_task_result(task).map(|res| res.unbind()))
    }
}
impl TaskCallback {
    fn get_task_result<'py>(&self, task: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        match task.call_method0(intern!(task.py(), "exception")) {
            Ok(task_exc) => {
                if task_exc.is_none() {
                    task.call_method0(intern!(task.py(), "result"))
                } else {
                    Err(PyErr::from_value(task_exc))
                }
            }
            Err(err) => Err(err),
        }
    }

    pub fn tx_send(&mut self, res: PyResult<Py<PyAny>>) -> PyResult<()> {
        self.tx
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("tx already consumed"))?
            .send(res)
            .map_err(|_| PyRuntimeError::new_err("Failed to send task result"))
    }
}
