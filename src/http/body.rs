use crate::asyncio::{PyCoroWaiter, py_coro_waiter};
use crate::client::{Client, TaskLocal};
use bytes::Bytes;
use futures_util::{FutureExt, Stream};
use pyo3::exceptions::PyRuntimeError;
use pyo3::intern;
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
            Some(InnerBody::Stream(stream)) => Ok(Some(&stream.stream)),
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
                let stream_repr = stream.stream.bind(py).repr()?;
                Ok(format!("BodyStream(stream={})", stream_repr.to_str()?))
            }
            None => Ok("Body(<already consumed>)".to_string()),
        }
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

    pub fn set_task_local(&mut self, py: Python, client: Option<&Client>) -> PyResult<()> {
        match self.body.as_mut() {
            Some(InnerBody::Bytes(_)) => Ok(()),
            Some(InnerBody::Stream(stream)) => stream.set_task_local(py, client),
            None => Err(PyRuntimeError::new_err("Body already consumed")),
        }
    }

    pub fn to_reqwest(&mut self) -> PyResult<reqwest::Body> {
        match self.body.take() {
            Some(InnerBody::Bytes(bytes)) => Ok(reqwest::Body::from(bytes)),
            Some(InnerBody::Stream(stream)) => stream.to_reqwest(),
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
    stream: Py<PyAny>,
    aiter: Py<PyAny>,
    task_local: Option<TaskLocal>,
    cur_waiter: Option<PyCoroWaiter>,
    started: bool,
    ellipsis: Py<PyEllipsis>,
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
                        if res.is(self.ellipsis.as_ref()) {
                            Poll::Ready(None) // Stream ended
                        } else {
                            Python::with_gil(|py| {
                                let bytes = res.extract::<PyBytes>(py)?;
                                Poll::Ready(Some(Ok(bytes)))
                            })
                        }
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
        static ONCE_ELLIPSIS: GILOnceCell<Py<PyEllipsis>> = GILOnceCell::new();
        let ellipsis = ONCE_ELLIPSIS.get_or_init(py, || PyEllipsis::get(py).into());

        static AITER: GILOnceCell<Py<PyAny>> = GILOnceCell::new();
        let aiter = AITER.import(py, "builtins", "aiter")?.call1((&stream,))?.unbind();

        Ok(BodyStream {
            stream,
            aiter,
            task_local: None,
            cur_waiter: None,
            started: false,
            ellipsis: ellipsis.clone_ref(py),
        })
    }

    pub fn to_reqwest(self) -> PyResult<reqwest::Body> {
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot use a stream that was already consumed"));
        }
        Ok(reqwest::Body::wrap_stream(self))
    }

    pub fn set_task_local(&mut self, py: Python, client: Option<&Client>) -> PyResult<()> {
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot set event loop after the stream has started"));
        }
        if self.task_local.is_none() {
            self.task_local = Some(Client::get_task_local_state(client, py)?);
        }
        Ok(())
    }

    fn py_anext(&mut self) -> PyResult<PyCoroWaiter> {
        static ONCE_ANEXT: GILOnceCell<Py<PyAny>> = GILOnceCell::new();

        self.started = true;

        Python::with_gil(|py| {
            let task_local = self
                .task_local
                .as_ref()
                .ok_or_else(|| PyRuntimeError::new_err("TaskLocal not set for BodyStream"))?;
            let anext = ONCE_ANEXT.import(py, "builtins", "anext")?;
            let coro = anext.call1((self.aiter.bind(py), &self.ellipsis))?;
            py_coro_waiter(coro, task_local)
        })
    }

    pub fn try_clone(&self, py: Python) -> PyResult<Self> {
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot clone a stream that was already consumed"));
        }
        let clone = self.stream.call_method0(py, intern!(py, "__copy__"))?;
        BodyStream::new(py, clone)
    }
}
