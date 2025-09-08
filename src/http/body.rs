use crate::asyncio::{PyCoroWaiter, TaskLocal, py_coro_waiter};
use bytes::Bytes;
use futures_util::{FutureExt, Stream};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::PyEllipsis;
use pyo3::{PyTraverseError, PyVisit, intern};
use pyo3_bytes::PyBytes;
use std::pin::Pin;
use std::sync::{Mutex, MutexGuard};
use std::task::{Context, Poll};

#[pyclass(frozen)]
pub struct RequestBody(Mutex<Option<InnerBody>>);

#[pymethods]
impl RequestBody {
    #[staticmethod]
    pub fn from_text(body: String) -> Self {
        Self::new(InnerBody::Bytes(body.into()))
    }

    #[staticmethod]
    pub fn from_bytes(body: PyBytes) -> Self {
        Self::new(InnerBody::Bytes(body.into_inner()))
    }

    #[staticmethod]
    pub fn from_stream(stream: Bound<PyAny>) -> PyResult<Self> {
        Ok(Self::new(InnerBody::Stream(BodyStream::new(stream)?)))
    }

    fn copy_bytes(&self) -> PyResult<Option<PyBytes>> {
        match self.lock()?.as_ref() {
            Some(InnerBody::Bytes(bytes)) => Ok(Some(PyBytes::from(bytes.clone()))),
            Some(InnerBody::Stream(_)) => Ok(None),
            None => Err(PyRuntimeError::new_err("RequestBody already consumed")),
        }
    }

    fn get_stream(&self, py: Python) -> PyResult<Option<Py<PyAny>>> {
        match self.lock()?.as_ref() {
            Some(InnerBody::Bytes(_)) => Ok(None),
            Some(InnerBody::Stream(stream)) => Ok(Some(stream.get_stream()?.clone_ref(py))),
            None => Err(PyRuntimeError::new_err("RequestBody already consumed")),
        }
    }

    fn __copy__(&self, py: Python) -> PyResult<Self> {
        self.try_clone(py)
    }

    pub fn __repr__(&self, py: Python) -> PyResult<String> {
        let type_name = py.get_type::<Self>().name()?;
        match self.lock()?.as_ref() {
            Some(InnerBody::Bytes(bytes)) => Ok(format!("{}(len={})", type_name, bytes.len())),
            Some(InnerBody::Stream(stream)) => {
                let stream_repr = stream.get_stream()?.bind(py).repr()?;
                Ok(format!("{}(stream={})", type_name, stream_repr.to_str()?))
            }
            None => Ok(format!("{}(<already consumed>)", type_name)),
        }
    }

    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        let Ok(inner) = self.lock() else {
            return Ok(());
        };
        match inner.as_ref() {
            Some(InnerBody::Bytes(_)) => Ok(()),
            Some(InnerBody::Stream(stream)) => stream.__traverse__(visit),
            None => Ok(()),
        }
    }
}
impl RequestBody {
    fn new(body: InnerBody) -> Self {
        Self(Mutex::new(Some(body)))
    }

    pub fn try_clone(&self, py: Python) -> PyResult<Self> {
        let body = match self.lock()?.as_ref() {
            Some(InnerBody::Bytes(bytes)) => InnerBody::Bytes(bytes.clone()),
            Some(InnerBody::Stream(stream)) => InnerBody::Stream(stream.try_clone(py)?),
            None => return Err(PyRuntimeError::new_err("RequestBody already consumed")),
        };
        Ok(Self::new(body))
    }

    pub fn take_inner(&self) -> PyResult<Self> {
        Ok(Self::new(
            self.lock()?
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("RequestBody already consumed"))?,
        ))
    }

    pub fn py_body(&self, py: Python) -> PyResult<Py<Self>> {
        Py::new(py, self.take_inner()?)
    }

    pub fn set_task_local(&self) -> PyResult<()> {
        match self.lock()?.as_mut() {
            Some(InnerBody::Bytes(_)) => Ok(()),
            Some(InnerBody::Stream(stream)) => stream.set_task_local(),
            None => Err(PyRuntimeError::new_err("RequestBody already consumed")),
        }
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn into_reqwest(&self) -> PyResult<reqwest::Body> {
        match self.lock()?.take() {
            Some(InnerBody::Bytes(bytes)) => Ok(reqwest::Body::from(bytes)),
            Some(InnerBody::Stream(stream)) => stream.into_reqwest(),
            None => Err(PyRuntimeError::new_err("RequestBody already consumed")),
        }
    }

    fn lock(&self) -> PyResult<MutexGuard<'_, Option<InnerBody>>> {
        self.0
            .lock()
            .map_err(|_| PyRuntimeError::new_err("RequestBody mutex poisoned"))
    }
}
impl From<Bytes> for RequestBody {
    fn from(bytes: Bytes) -> Self {
        Self::new(InnerBody::Bytes(bytes))
    }
}

enum InnerBody {
    Bytes(Bytes),
    Stream(BodyStream),
}

pub struct BodyStream {
    stream: Option<Py<PyAny>>,
    py_iter: Option<Py<PyAny>>,
    task_local: Option<TaskLocal>,
    cur_waiter: Option<StreamWaiter>,
    started: bool,
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
            started: false,
        })
    }

    pub fn get_stream(&self) -> PyResult<&Py<PyAny>> {
        self.stream
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Expected stream"))
    }

    pub fn into_reqwest(self) -> PyResult<reqwest::Body> {
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot use a stream that was already consumed"));
        }
        Ok(reqwest::Body::wrap_stream(self))
    }

    pub fn set_task_local(&mut self) -> PyResult<()> {
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot set event loop after the stream has started"));
        }
        if self.is_async && self.task_local.is_none() {
            self.task_local = Some(Python::attach(TaskLocal::current)?);
        }
        Ok(())
    }

    fn py_next(&mut self) -> PyResult<StreamWaiter> {
        self.started = true;

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
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot clone a stream that was already consumed"));
        }

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
            started: false,
        })
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
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
    }
}

fn is_async_iter(obj: &Bound<PyAny>) -> PyResult<bool> {
    static ASYNC_TYPE: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    obj.is_instance(ASYNC_TYPE.import(obj.py(), "collections.abc", "AsyncIterable")?)
}

enum StreamWaiter {
    Async(PyCoroWaiter),
    Sync(Option<Py<PyAny>>),
}
