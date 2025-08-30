use crate::asyncio::{PyCoroWaiter, py_coro_waiter, TaskLocal};
use bytes::Bytes;
use futures_util::{FutureExt, Stream};
use pyo3::exceptions::PyRuntimeError;
use pyo3::{intern, PyTraverseError, PyVisit};
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::PyEllipsis;
use pyo3_bytes::PyBytes;
use std::pin::Pin;
use std::task::{Context, Poll};

#[pyclass]
pub struct Body {
    body: Option<InnerBody>,
}
#[pymethods]
impl Body {
    #[staticmethod]
    pub fn from_text(body: String) -> Self {
        Body {
            body: Some(InnerBody::Bytes(body.into())),
        }
    }

    #[staticmethod]
    pub fn from_bytes(body: PyBytes) -> Self {
        Body {
            body: Some(InnerBody::Bytes(body.into_inner())),
        }
    }

    #[staticmethod]
    pub fn from_stream(py: Python, stream: Py<PyAny>) -> PyResult<Self> {
        Ok(Body {
            body: Some(InnerBody::Stream(BodyStream::new(py, stream)?)),
        })
    }

    fn copy_bytes(&self) -> PyResult<Option<PyBytes>> {
        match self.body.as_ref() {
            Some(InnerBody::Bytes(bytes)) => Ok(Some(PyBytes::from(bytes.clone()))),
            Some(InnerBody::Stream(_)) => Ok(None),
            None => Err(PyRuntimeError::new_err("Body already consumed")),
        }
    }

    fn get_stream(&self) -> PyResult<Option<&Py<PyAny>>> {
        match self.body.as_ref() {
            Some(InnerBody::Bytes(_)) => Ok(None),
            Some(InnerBody::Stream(stream)) => Ok(Some(stream.get_stream()?)),
            None => Err(PyRuntimeError::new_err("Body already consumed")),
        }
    }

    fn __copy__(&self, py: Python) -> PyResult<Self> {
        self.try_clone(py)
    }

    pub fn __repr__(&self, py: Python) -> PyResult<String> {
        match &self.body {
            Some(InnerBody::Bytes(bytes)) => Ok(format!("BodyBytes(len={})", bytes.len())),
            Some(InnerBody::Stream(stream)) => {
                let stream_repr = stream.get_stream()?.bind(py).repr()?;
                Ok(format!("BodyStream(stream={})", stream_repr.to_str()?))
            }
            None => Ok("Body(<already consumed>)".to_string()),
        }
    }

    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        match &self.body {
            Some(InnerBody::Bytes(_)) => Ok(()),
            Some(InnerBody::Stream(stream)) => stream.__traverse__(visit),
            None => Ok(()),
        }
    }

    pub fn __clear__(&mut self) {
        self.body = None;
    }
}
impl Body {
    pub fn try_clone(&self, py: Python) -> PyResult<Self> {
        let body = match &self.body {
            Some(InnerBody::Bytes(bytes)) => InnerBody::Bytes(bytes.clone()),
            Some(InnerBody::Stream(stream)) => InnerBody::Stream(stream.try_clone(py)?),
            None => return Err(PyRuntimeError::new_err("Body already consumed")),
        };
        Ok(Body { body: Some(body) })
    }

    pub fn take_inner(&mut self) -> PyResult<Body> {
        match self.body.take() {
            Some(inner) => Ok(Body { body: Some(inner) }),
            None => Err(PyRuntimeError::new_err("Body already consumed")),
        }
    }

    pub fn set_task_local(&mut self, py: Python) -> PyResult<()> {
        match self.body.as_mut() {
            Some(InnerBody::Bytes(_)) => Ok(()),
            Some(InnerBody::Stream(stream)) => stream.set_task_local(py),
            None => Err(PyRuntimeError::new_err("Body already consumed")),
        }
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn into_reqwest(&mut self) -> PyResult<reqwest::Body> {
        match self.body.take() {
            Some(InnerBody::Bytes(bytes)) => Ok(reqwest::Body::from(bytes)),
            Some(InnerBody::Stream(stream)) => stream.into_reqwest(),
            None => Err(PyRuntimeError::new_err("Body already consumed")),
        }
    }
}
impl From<Vec<u8>> for Body {
    fn from(bytes: Vec<u8>) -> Self {
        Body {
            body: Some(InnerBody::Bytes(bytes.into())),
        }
    }
}

enum InnerBody {
    Bytes(Bytes),
    Stream(BodyStream),
}

pub struct BodyStream {
    stream: Option<Py<PyAny>>,
    aiter: Option<Py<PyAny>>,
    task_local: Option<TaskLocal>,
    cur_waiter: Option<PyCoroWaiter>,
    started: bool,
}
impl Stream for BodyStream {
    type Item = PyResult<PyBytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.cur_waiter.is_none() {
            self.cur_waiter = match self.py_anext() {
                Ok(waiter) => Some(waiter),
                Err(e) => return Poll::Ready(Some(Err(e))),
            };
        }

        match self.cur_waiter.as_mut().unwrap().poll_unpin(cx) {
            Poll::Ready(res) => {
                self.cur_waiter = None;
                match res {
                    Ok(res) => {
                        Python::with_gil(|py| {
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
    pub fn new(py: Python, stream: Py<PyAny>) -> PyResult<Self> {
        Ok(BodyStream {
            aiter: Some(Self::get_aiter(py, &stream)?.unbind()),
            stream: Some(stream),
            task_local: None,
            cur_waiter: None,
            started: false,
        })
    }

    pub fn get_stream(&self) -> PyResult<&Py<PyAny>> {
        self.stream.as_ref().ok_or_else(|| PyRuntimeError::new_err("Expected stream"))
    }

    pub fn into_reqwest(self) -> PyResult<reqwest::Body> {
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot use a stream that was already consumed"));
        }
        Ok(reqwest::Body::wrap_stream(self))
    }

    pub fn set_task_local(&mut self, py: Python) -> PyResult<()> {
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot set event loop after the stream has started"));
        }
        if self.task_local.is_none() {
            self.task_local = Some(TaskLocal::current(py)?);
        }
        Ok(())
    }

    fn py_anext(&mut self) -> PyResult<PyCoroWaiter> {
        self.started = true;

        Python::with_gil(|py| {
            let aiter = self
                .aiter
                .as_ref()
                .ok_or_else(|| PyRuntimeError::new_err("Expected aiter"))?;

            let task_local = self
                .task_local
                .as_ref()
                .ok_or_else(|| PyRuntimeError::new_err("Expected task_local"))?;

            py_coro_waiter(self.anext_coro(py, aiter)?, task_local)
        })
    }

    fn get_aiter<'py>(py: Python<'py>, stream: &Py<PyAny>) -> PyResult<Bound<'py, PyAny>> {
        static AITER: GILOnceCell<Py<PyAny>> = GILOnceCell::new();
        AITER.import(py, "builtins", "aiter")?.call1((stream,))
    }

    fn anext_coro<'py>(&self, py: Python<'py>, aiter: &Py<PyAny>) -> PyResult<Bound<'py, PyAny>> {
        static ANEXT: GILOnceCell<Py<PyAny>> = GILOnceCell::new();
        ANEXT.import(py, "builtins", "anext")?.call1((aiter, self.ellipsis(py)))
    }

    fn ellipsis(&self, py: Python) -> &Py<PyEllipsis> {
        static ONCE_ELLIPSIS: GILOnceCell<Py<PyEllipsis>> = GILOnceCell::new();
        ONCE_ELLIPSIS.get_or_init(py, || PyEllipsis::get(py).into())
    }

    fn is_end_marker(&self, py: Python, obj: &Py<PyAny>) -> bool {
        obj.is(self.ellipsis(py))
    }

    pub fn try_clone(&self, py: Python) -> PyResult<Self> {
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot clone a stream that was already consumed"));
        }

        let new_stream = self.stream.as_ref().ok_or_else(|| PyRuntimeError::new_err("Expected stream"))?.call_method0(py, intern!(py, "__copy__"))?;

        Ok(BodyStream {
            aiter: Some(Self::get_aiter(py, &new_stream)?.unbind()),
            stream: Some(new_stream),
            task_local: None,
            cur_waiter: None,
            started: false,
        })
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.stream)?;
        visit.call(&self.aiter)?;
        self.task_local.as_ref().map(|v| v.__traverse__(visit)).transpose()?;
        Ok(())
    }

    fn __clear__(&mut self) {
        self.stream = None;
        self.aiter = None;
        self.task_local = None;
        self.cur_waiter = None;
    }
}
