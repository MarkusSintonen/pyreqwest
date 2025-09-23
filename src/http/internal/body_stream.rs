use crate::asyncio::{PyCoroWaiter, TaskLocal, py_coro_waiter};
use futures_util::{FutureExt, Stream};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::PyEllipsis;
use pyo3::{PyTraverseError, PyVisit, intern};
use pyo3_bytes::PyBytes;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct BodyStream {
    stream: Option<Py<PyAny>>,
    py_iter: Option<Py<PyAny>>,
    task_local: Option<TaskLocal>,
    cur_waiter: Option<StreamWaiter>,
    is_async: bool,
}
impl Stream for BodyStream {
    type Item = PyResult<PyBytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.cur_waiter.is_none() {
            self.cur_waiter = match self.py_next() {
                Ok(waiter) => Some(waiter),
                Err(e) => return Poll::Ready(Some(Err(e))),
            };
        }

        let poll_res = match self.cur_waiter.as_mut().unwrap() {
            StreamWaiter::Async(waiter) => waiter.poll_unpin(cx),
            StreamWaiter::Sync(obj) => Poll::Ready(
                obj.take()
                    .ok_or_else(|| PyRuntimeError::new_err("Unexpected missing stream value")),
            ),
        };

        match poll_res {
            Poll::Ready(res) => {
                self.cur_waiter = None;
                match res {
                    Ok(res) => {
                        Python::attach(|py| {
                            if self.is_end_marker(py, &res) {
                                Poll::Ready(None) // Stream ended
                            } else {
                                let bytes = res.extract::<PyBytes>(py)?;
                                Poll::Ready(Some(Ok(bytes)))
                            }
                        })
                    }
                    Err(e) => Poll::Ready(Some(Err(e))),
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
impl BodyStream {
    pub fn new(stream: Bound<PyAny>) -> PyResult<Self> {
        let is_async = is_async_iter(&stream)?;
        Ok(BodyStream {
            is_async,
            py_iter: Some(Self::get_py_iter(&stream, is_async)?.unbind()),
            stream: Some(stream.unbind()),
            task_local: None,
            cur_waiter: None,
        })
    }

    pub fn is_async(&self) -> bool {
        self.is_async
    }

    pub fn get_stream(&self) -> PyResult<&Py<PyAny>> {
        self.stream
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Expected stream"))
    }

    pub fn into_reqwest(self, is_blocking: bool) -> PyResult<reqwest::Body> {
        if is_blocking && self.is_async {
            return Err(PyValueError::new_err("Cannot use async iterator in a blocking context"));
        }
        Ok(reqwest::Body::wrap_stream(self))
    }

    pub fn set_task_local(&mut self) -> PyResult<()> {
        if self.is_async && self.task_local.is_none() {
            self.task_local = Some(Python::attach(TaskLocal::current)?);
        }
        Ok(())
    }

    fn py_next(&mut self) -> PyResult<StreamWaiter> {
        let py_iter = self
            .py_iter
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Expected iterator"))?;

        Python::attach(|py| {
            if self.is_async {
                let task_local = match self.task_local.as_ref() {
                    Some(tl) => tl,
                    None => &TaskLocal::current(py)?,
                };

                static ANEXT: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
                let coro = ANEXT
                    .import(py, "builtins", "anext")?
                    .call1((py_iter, self.ellipsis(py)))?;
                Ok(StreamWaiter::Async(py_coro_waiter(coro, task_local)?))
            } else {
                static NEXT: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
                let res = NEXT
                    .import(py, "builtins", "next")?
                    .call1((py_iter, self.ellipsis(py)))?;
                Ok(StreamWaiter::Sync(Some(res.unbind())))
            }
        })
    }

    fn get_py_iter<'py>(stream: &Bound<'py, PyAny>, is_async: bool) -> PyResult<Bound<'py, PyAny>> {
        if is_async {
            static AITER: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
            AITER.import(stream.py(), "builtins", "aiter")?.call1((stream,))
        } else {
            static ITER: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
            ITER.import(stream.py(), "builtins", "iter")?.call1((stream,))
        }
    }

    fn ellipsis(&self, py: Python) -> &Py<PyEllipsis> {
        static ONCE_ELLIPSIS: PyOnceLock<Py<PyEllipsis>> = PyOnceLock::new();
        ONCE_ELLIPSIS.get_or_init(py, || PyEllipsis::get(py).into())
    }

    fn is_end_marker(&self, py: Python, obj: &Py<PyAny>) -> bool {
        obj.is(self.ellipsis(py))
    }

    pub fn try_clone(&self, py: Python) -> PyResult<Self> {
        let new_stream = self
            .stream
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Expected stream"))?
            .bind(py)
            .call_method0(intern!(py, "__copy__"))?;

        Ok(BodyStream {
            is_async: self.is_async,
            py_iter: Some(Self::get_py_iter(&new_stream, self.is_async)?.unbind()),
            stream: Some(new_stream.unbind()),
            task_local: None,
            cur_waiter: None,
        })
    }

    // :NOCOV_START
    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.stream)?;
        visit.call(&self.py_iter)?;
        self.task_local.as_ref().map(|v| v.__traverse__(&visit)).transpose()?;
        Ok(())
    }

    fn __clear__(&mut self) {
        self.stream = None;
        self.py_iter = None;
        self.task_local = None;
        self.cur_waiter = None;
    } // :NOCOV_END
}

fn is_async_iter(obj: &Bound<PyAny>) -> PyResult<bool> {
    static ASYNC_TYPE: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    obj.is_instance(ASYNC_TYPE.import(obj.py(), "collections.abc", "AsyncIterable")?)
}

enum StreamWaiter {
    Async(PyCoroWaiter),
    Sync(Option<Py<PyAny>>),
}
