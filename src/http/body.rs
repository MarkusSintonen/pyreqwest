use crate::asyncio::{EventLoopCell, PyCoroWaiter, py_coro_waiter};
use bytes::Bytes;
use futures_util::{FutureExt, Stream};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::PyNone;
use pyo3_bytes::PyBytes;
use std::pin::Pin;
use std::task::{Context, Poll};

#[pyclass]
pub struct Body {
    body: InnerBody,
}
#[pymethods]
impl Body {
    #[staticmethod]
    pub fn from_str(body: String) -> Self {
        Body {
            body: InnerBody::Bytes(body.into()),
        }
    }

    #[staticmethod]
    pub fn from_bytes(body: PyBytes) -> Self {
        Body {
            body: InnerBody::Bytes(body.into_inner()),
        }
    }

    #[staticmethod]
    pub fn from_stream(async_gen: Py<PyAny>) -> Self {
        Body {
            body: InnerBody::Stream(BodyStream::new(async_gen)),
        }
    }

    fn get_bytes(&self) -> Option<PyBytes> {
        match &self.body {
            InnerBody::Bytes(bytes) => Some(PyBytes::from(bytes.clone())),
            InnerBody::Stream(_) => None,
        }
    }

    fn get_stream(&self) -> Option<&Py<PyAny>> {
        match &self.body {
            InnerBody::Bytes(_) => None,
            InnerBody::Stream(stream) => Some(&stream.async_gen),
        }
    }
}
impl TryInto<reqwest::Body> for Body {
    type Error = PyErr;
    fn try_into(self) -> PyResult<reqwest::Body> {
        match self.body {
            InnerBody::Bytes(bytes) => Ok(reqwest::Body::from(bytes)),
            InnerBody::Stream(stream) => {
                if stream.started {
                    return Err(PyRuntimeError::new_err("Cannot use a stream that was already consumed"));
                }
                Ok(reqwest::Body::wrap_stream(stream))
            }
        }
    }
}
impl Body {
    pub fn try_clone(&self) -> PyResult<Self> {
        let body = match &self.body {
            InnerBody::Bytes(bytes) => InnerBody::Bytes(bytes.clone()),
            InnerBody::Stream(stream) => InnerBody::Stream(stream.try_clone()?),
        };
        Ok(Body { body })
    }

    pub fn set_stream_event_loop(&mut self, py: Python, ev_loop: &mut EventLoopCell) -> PyResult<()> {
        if let InnerBody::Stream(stream) = &mut self.body {
            stream.set_event_loop(ev_loop.get_running_loop(py)?.clone_ref(py))?;
        }
        Ok(())
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
                Poll::Ready(
                    res.and_then(|r| Python::with_gil(|py| r.extract::<Option<PyBytes>>(py)))
                        .transpose(),
                )
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
impl BodyStream {
    pub fn new(async_gen: Py<PyAny>) -> Self {
        BodyStream {
            async_gen,
            event_loop: None,
            cur_waiter: None,
            started: false,
        }
    }

    pub fn set_event_loop(&mut self, event_loop: Py<PyAny>) -> PyResult<()> {
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot set event loop after the stream has started"));
        }
        self.event_loop = Some(event_loop);
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
            let coro = anext.call1((self.async_gen.bind(py), PyNone::get(py)))?;
            py_coro_waiter(&coro, event_loop.bind(py))
        })
    }

    pub fn try_clone(&self) -> PyResult<Self> {
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot clone a stream that was already consumed"));
        }
        Ok(BodyStream::new(Python::with_gil(|py| self.async_gen.clone_ref(py))))
    }
}
