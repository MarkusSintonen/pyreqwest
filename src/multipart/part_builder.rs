use crate::allow_threads::AllowThreads;
use crate::client::RuntimeHandle;
use crate::http::internal::body_stream::BodyStream;
use crate::http::{HeaderMap, MimeType};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3_bytes::PyBytes;
use std::path::PathBuf;

#[pyclass]
pub struct PartBuilder {
    inner: Option<reqwest::multipart::Part>,
    is_async: bool,
}
#[pymethods]
impl PartBuilder {
    #[staticmethod]
    fn from_text(py: Python, value: String) -> Self {
        py.detach(|| Self::new(reqwest::multipart::Part::text(value), false))
    }

    #[staticmethod]
    fn from_bytes(py: Python, value: PyBytes) -> Self {
        py.detach(|| Self::new(reqwest::multipart::Part::bytes(Vec::from(value.into_inner())), false))
    }

    #[staticmethod]
    fn from_stream(py: Python, stream: Bound<PyAny>) -> PyResult<Self> {
        let mut stream = BodyStream::new(stream)?;
        stream.set_task_local()?;
        let is_async = stream.is_async();
        py.detach(|| Ok(Self::new(reqwest::multipart::Part::stream(stream.into_reqwest(false)?), is_async)))
    }

    #[staticmethod]
    fn from_stream_with_length(py: Python, stream: Bound<PyAny>, length: u64) -> PyResult<Self> {
        let mut stream = BodyStream::new(stream)?;
        stream.set_task_local()?;
        let is_async = stream.is_async();
        py.detach(|| {
            Ok(Self::new(
                reqwest::multipart::Part::stream_with_length(stream.into_reqwest(false)?, length),
                is_async,
            ))
        })
    }

    #[staticmethod]
    async fn from_file(path: PathBuf, #[pyo3(cancel_handle)] cancel: CancelHandle) -> PyResult<Self> {
        let fut = RuntimeHandle::global_handle()?.spawn_handled(reqwest::multipart::Part::file(path), cancel);
        let part = AllowThreads(fut).await??;
        Ok(Self::new(part, false))
    }

    #[staticmethod]
    fn from_sync_file(path: PathBuf) -> PyResult<Self> {
        let part = RuntimeHandle::global_handle()?.blocking_spawn(reqwest::multipart::Part::file(path))?;
        Ok(Self::new(part, false))
    }

    fn mime<'py>(slf: PyRefMut<'py, Self>, mime: MimeType) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| {
            builder
                .mime_str(mime.0.as_ref())
                .map_err(|e| PyValueError::new_err(e.to_string()))
        })
    }

    fn file_name(slf: PyRefMut<'_, Self>, filename: String) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |builder| Ok(builder.file_name(filename)))
    }

    fn headers(slf: PyRefMut<'_, Self>, headers: HeaderMap) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |builder| Ok(builder.headers(headers.try_take_inner()?)))
    }
}
impl PartBuilder {
    fn new(part: reqwest::multipart::Part, is_async: bool) -> Self {
        PartBuilder {
            inner: Some(part),
            is_async,
        }
    }

    pub fn build(&mut self) -> PyResult<reqwest::multipart::Part> {
        self.inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Part was already consumed"))
    }

    pub fn is_async(&self) -> bool {
        self.is_async
    }

    fn apply<F>(mut slf: PyRefMut<Self>, fun: F) -> PyResult<PyRefMut<Self>>
    where
        F: FnOnce(reqwest::multipart::Part) -> PyResult<reqwest::multipart::Part>,
        F: Send,
    {
        let builder = slf
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Part was already consumed"))?;
        slf.inner = Some(slf.py().detach(|| fun(builder))?);
        Ok(slf)
    }
}
