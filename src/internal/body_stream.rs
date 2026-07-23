use crate::internal::asyncio_coro::BytesCoroWaiter;
use crate::internal::task_local::{OnceTaskLocal, TaskLocal};
use bytes::Bytes;
use futures_util::{FutureExt, Stream};
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
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
    type Item = PyResult<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.cur_waiter.is_none() {
            self.cur_waiter = match self.py_next() {
                Ok(waiter) => Some(waiter),
                Err(e) => return Poll::Ready(Some(Err(e))),
            };
        }

        let poll_res = match self.cur_waiter.as_mut() {
            Some(StreamWaiter::Async(waiter)) => waiter.poll_unpin(cx),
            Some(StreamWaiter::Sync(obj)) => Poll::Ready(
                obj.take()
                    .ok_or_else(|| PyRuntimeError::new_err("Unexpected missing stream value"))
                    .flatten(),
            ),
            None => unreachable!("cur_waiter should be Some here"),
        };

        match poll_res {
            Poll::Ready(res) => {
                self.cur_waiter = None;
                match res {
                    Ok(Some(res)) => Poll::Ready(Some(Ok(res))),
                    Ok(None) => Poll::Ready(None), // End of stream
                    Err(e) => Poll::Ready(Some(Err(e))),
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
impl BodyStream {
    pub fn new(stream: Bound<PyAny>) -> PyResult<Self> {
        let is_async = !is_sync_iter(&stream)?;
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

    pub fn set_task_local(&mut self, py: Python, task_local: &OnceTaskLocal) -> PyResult<()> {
        if self.is_async && self.task_local.is_none() {
            self.task_local = Some(task_local.get_or_current(py)?);
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
                    .call1((py_iter, Self::sentinel(py)?))?;
                Ok(StreamWaiter::Async(BytesCoroWaiter::new(
                    coro,
                    Box::new(Self::bytes_extractor),
                    task_local,
                    None,
                )?))
            } else {
                static NEXT: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
                let res = NEXT
                    .import(py, "builtins", "next")?
                    .call1((py_iter, Self::sentinel(py)?));
                Ok(StreamWaiter::Sync(Some(Self::bytes_extractor(py, res))))
            }
        })
    }

    fn bytes_extractor(py: Python, res: PyResult<Bound<PyAny>>) -> PyResult<Option<Bytes>> {
        let res = res?;
        if res.is(Self::sentinel(py)?) {
            return Ok(None); // End of stream
        }
        let py_bytes = {
            let b = res.extract::<PyBytes>()?;
            b.into_inner()
        };
        let bytes = Bytes::copy_from_slice(&py_bytes);
        // Drop of PyBytes and backing PyBuffer requires GIL, so do it before returning from GIL context. This avoids
        // reacquiring GIL deeper inside request processing.
        drop(py_bytes);
        Ok(Some(bytes))
    }

    fn get_py_iter<'py>(stream: &Bound<'py, PyAny>, is_async: bool) -> PyResult<Bound<'py, PyAny>> {
        let iter_init = if is_async {
            static AITER: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
            AITER.import(stream.py(), "builtins", "aiter")?
        } else {
            static ITER: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
            ITER.import(stream.py(), "builtins", "iter")?
        };
        iter_init.call1((stream,)).map_err(|e| {
            if !(is_async_iter(stream).unwrap_or_default() || is_sync_iter(stream).unwrap_or_default()) {
                PyTypeError::new_err("Expected an iterable or async iterable")
            } else {
                e
            }
        })
    }

    fn sentinel(py: Python<'_>) -> PyResult<&Py<PyAny>> {
        static SENTINEL: PyOnceLock<PyResult<Py<PyAny>>> = PyOnceLock::new();
        match SENTINEL.get_or_init(py, || Sentinel::new_py(py)) {
            Ok(s) => Ok(s),
            Err(e) => Err(e.clone_ref(py)),
        }
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
    static TYPE: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    obj.is_instance(TYPE.import(obj.py(), "collections.abc", "AsyncIterable")?)
}

fn is_sync_iter(obj: &Bound<PyAny>) -> PyResult<bool> {
    static TYPE: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    obj.is_instance(TYPE.import(obj.py(), "collections.abc", "Iterable")?)
}

enum StreamWaiter {
    Async(BytesCoroWaiter),
    Sync(Option<PyResult<Option<Bytes>>>),
}

#[pyclass(frozen)]
struct Sentinel;
impl Sentinel {
    fn new_py(py: Python) -> PyResult<Py<PyAny>> {
        Ok(Py::new(py, Sentinel)?.into_any())
    }
}
