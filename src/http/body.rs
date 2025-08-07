use crate::asyncio::{PyCoroWaiter, py_coro_waiter};
use crate::client::Client;
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
    pub fn from_stream(py: Python, async_gen: Py<PyAny>) -> Self {
        Body {
            body: Some(InnerBody::Stream(BodyStream::new(py, async_gen))),
        }
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
            Some(InnerBody::Stream(stream)) => Ok(Some(&stream.async_gen)),
            None => Err(PyRuntimeError::new_err("Body already consumed")),
        }
    }

    fn __copy__(&self, py: Python) -> PyResult<Self> {
        self.try_clone(py)
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

    pub fn set_stream_event_loop(&mut self, py: Python, client: &Client) -> PyResult<()> {
        match self.body.as_mut() {
            Some(InnerBody::Bytes(_)) => Ok(()),
            Some(InnerBody::Stream(stream)) => stream.set_event_loop(py, client),
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

enum InnerBody {
    Bytes(Bytes),
    Stream(BodyStream),
}

pub struct BodyStream {
    async_gen: Py<PyAny>,
    event_loop: Option<Py<PyAny>>,
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
    pub fn new(py: Python, async_gen: Py<PyAny>) -> Self {
        static ONCE_ELLIPSIS: GILOnceCell<Py<PyEllipsis>> = GILOnceCell::new();
        let ellipsis = ONCE_ELLIPSIS.get_or_init(py, || PyEllipsis::get(py).into());

        BodyStream {
            async_gen,
            event_loop: None,
            cur_waiter: None,
            started: false,
            ellipsis: ellipsis.clone_ref(py),
        }
    }

    pub fn to_reqwest(self) -> PyResult<reqwest::Body> {
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot use a stream that was already consumed"));
        }
        Ok(reqwest::Body::wrap_stream(self))
    }

    pub fn set_event_loop(&mut self, py: Python, client: &Client) -> PyResult<()> {
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot set event loop after the stream has started"));
        }
        self.event_loop = Some(client.get_event_loop().get_running_loop(py)?.clone_ref(py));
        Ok(())
    }

    fn py_anext(&mut self) -> PyResult<PyCoroWaiter> {
        static ONCE_ANEXT: GILOnceCell<Py<PyAny>> = GILOnceCell::new();

        self.started = true;

        Python::with_gil(|py| {
            let event_loop = self
                .event_loop
                .as_ref()
                .ok_or_else(|| PyRuntimeError::new_err("Event loop not set for BodyStream"))?;
            let anext = ONCE_ANEXT.import(py, "builtins", "anext")?;
            let coro = anext.call1((self.async_gen.bind(py), &self.ellipsis))?;
            py_coro_waiter(coro, event_loop.bind(py))
        })
    }

    pub fn try_clone(&self, py: Python) -> PyResult<Self> {
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot clone a stream that was already consumed"));
        }
        let clone = self.async_gen.call_method0(py, intern!(py, "__copy__"))?;
        Ok(BodyStream::new(py, clone))
    }
}
