use crate::asyncio::{PyCoroWaiter, py_coro_waiter};
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
pub struct RequestBody {
    body: Body,
}
#[pymethods]
impl RequestBody {
    #[staticmethod]
    pub fn from_str(body: String) -> Self {
        RequestBody {
            body: Body::Bytes(body.into()),
        }
    }

    #[staticmethod]
    pub fn from_bytes(body: PyBytes) -> Self {
        RequestBody {
            body: Body::Bytes(body.into_inner()),
        }
    }

    #[staticmethod]
    pub fn from_stream(async_gen: Py<PyAny>) -> Self {
        RequestBody {
            body: Body::Stream(BodyStream::new(async_gen)),
        }
    }

    fn get_bytes(&self) -> Option<PyBytes> {
        match &self.body {
            Body::Bytes(bytes) => Some(PyBytes::from(bytes.clone())),
            Body::Stream(_) => None,
        }
    }

    fn get_stream(&self) -> Option<&Py<PyAny>> {
        match &self.body {
            Body::Bytes(_) => None,
            Body::Stream(stream) => Some(&stream.async_gen),
        }
    }
}
impl TryInto<reqwest::Body> for RequestBody {
    type Error = PyErr;
    fn try_into(self) -> PyResult<reqwest::Body> {
        match self.body {
            Body::Bytes(bytes) => Ok(reqwest::Body::from(bytes)),
            Body::Stream(stream) => {
                if stream.started {
                    return Err(PyRuntimeError::new_err("Cannot use a stream that was already consumed"));
                }
                Ok(reqwest::Body::wrap_stream(stream))
            }
        }
    }
}
impl RequestBody {
    pub fn try_clone(&self) -> PyResult<Self> {
        let body = match &self.body {
            Body::Bytes(bytes) => Body::Bytes(bytes.clone()),
            Body::Stream(stream) => Body::Stream(stream.try_clone()?),
        };
        Ok(RequestBody { body })
    }
}

enum Body {
    Bytes(Bytes),
    Stream(BodyStream),
}

pub struct BodyStream {
    async_gen: Py<PyAny>,
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
            cur_waiter: None,
            started: false,
        }
    }

    fn py_anext(&mut self) -> PyResult<PyCoroWaiter> {
        static ONCE_ANEXT: GILOnceCell<Py<PyAny>> = GILOnceCell::new();

        self.started = true;

        Python::with_gil(|py| {
            let anext = ONCE_ANEXT.import(py, "builtins", "anext")?;
            let coro = anext.call((self.async_gen.bind(py), PyNone::get(py)), None)?;
            py_coro_waiter(py, coro)
        })
    }

    pub fn try_clone(&self) -> PyResult<Self> {
        if self.started {
            return Err(PyRuntimeError::new_err("Cannot clone a stream that was already consumed"));
        }
        Ok(BodyStream::new(Python::with_gil(|py| self.async_gen.clone_ref(py))))
    }
}
