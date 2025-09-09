use crate::allow_threads::AllowThreads;
use crate::client::Handle;
use crate::http::{BodyStream, HeaderMap};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3_bytes::PyBytes;
use std::path::PathBuf;

#[pyclass]
pub struct PartBuilder {
    inner: Option<reqwest::multipart::Part>,
}
#[pymethods]
impl PartBuilder {
    #[staticmethod]
    fn from_text(py: Python, value: String) -> Self {
        py.detach(|| reqwest::multipart::Part::text(value).into())
    }

    #[staticmethod]
    fn from_bytes(py: Python, value: PyBytes) -> Self {
        py.detach(|| reqwest::multipart::Part::bytes(Vec::from(value.into_inner())).into())
    }

    #[staticmethod]
    fn from_stream(py: Python, stream: Bound<PyAny>) -> PyResult<Self> {
        let mut stream = BodyStream::new(stream)?;
        stream.set_task_local()?;
        py.detach(|| Ok(reqwest::multipart::Part::stream(stream.into_reqwest()?).into()))
    }

    #[staticmethod]
    fn from_stream_with_length(py: Python, stream: Bound<PyAny>, length: u64) -> PyResult<Self> {
        let mut stream = BodyStream::new(stream)?;
        stream.set_task_local()?;
        py.detach(|| Ok(reqwest::multipart::Part::stream_with_length(stream.into_reqwest()?, length).into()))
    }

    #[staticmethod]
    async fn from_file(path: PathBuf, #[pyo3(cancel_handle)] cancel: CancelHandle) -> PyResult<Self> {
        let fut = Handle::global_handle()?.spawn_handled(reqwest::multipart::Part::file(path), cancel);
        let part = AllowThreads(fut).await??;
        Ok(part.into())
    }

    #[staticmethod]
    fn blocking_from_file(path: PathBuf) -> PyResult<Self> {
        let part = Handle::global_handle()?.blocking_spawn(reqwest::multipart::Part::file(path))?;
        Ok(part.into())
    }

    fn mime_str<'py>(slf: PyRefMut<'py, Self>, mime: &str) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| builder.mime_str(mime).map_err(|e| PyValueError::new_err(e.to_string())))
    }

    fn file_name(slf: PyRefMut<'_, Self>, filename: String) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |builder| Ok(builder.file_name(filename)))
    }

    fn headers(slf: PyRefMut<'_, Self>, headers: HeaderMap) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |builder| Ok(builder.headers(headers.try_take_inner()?)))
    }
}
impl PartBuilder {
    pub fn build(&mut self) -> PyResult<reqwest::multipart::Part> {
        self.inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Part was already consumed"))
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
impl From<reqwest::multipart::Part> for PartBuilder {
    fn from(part: reqwest::multipart::Part) -> Self {
        PartBuilder { inner: Some(part) }
    }
}
