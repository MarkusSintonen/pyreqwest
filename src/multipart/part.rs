use crate::client::Runtime;
use crate::http::{BodyStream, HeaderMap};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3_bytes::PyBytes;
use std::path::PathBuf;

#[pyclass]
pub struct Part {
    inner: Option<reqwest::multipart::Part>,
}
#[pymethods]
impl Part {
    #[staticmethod]
    fn from_text(value: String) -> Self {
        reqwest::multipart::Part::text(value).into()
    }

    #[staticmethod]
    fn from_bytes(value: PyBytes) -> Self {
        reqwest::multipart::Part::bytes(Vec::from(value.into_inner())).into()
    }

    #[staticmethod]
    fn from_stream(py: Python, async_gen: Py<PyAny>) -> PyResult<Self> {
        let mut stream = BodyStream::new(py, async_gen);
        stream.set_task_local(py, None)?;
        Ok(reqwest::multipart::Part::stream(stream.to_reqwest()?).into())
    }

    #[staticmethod]
    fn from_stream_with_length(py: Python, async_gen: Py<PyAny>, length: u64) -> PyResult<Self> {
        let mut stream = BodyStream::new(py, async_gen);
        stream.set_task_local(py, None)?;
        Ok(reqwest::multipart::Part::stream_with_length(stream.to_reqwest()?, length).into())
    }

    #[staticmethod]
    async fn from_file(path: PathBuf, #[pyo3(cancel_handle)] cancel: CancelHandle) -> PyResult<Self> {
        let part = Runtime::global_handle()?
            .spawn(reqwest::multipart::Part::file(path), cancel)
            .await??;
        Ok(part.into())
    }

    fn mime_str<'py>(slf: PyRefMut<'py, Self>, mime: &str) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| builder.mime_str(mime).map_err(|e| PyValueError::new_err(e.to_string())))
    }

    fn file_name<'py>(slf: PyRefMut<'py, Self>, filename: String) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| Ok(builder.file_name(filename)))
    }

    fn headers<'py>(slf: PyRefMut<'py, Self>, mut headers: HeaderMap) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| Ok(builder.headers(headers.try_take_inner()?)))
    }
}
impl Part {
    fn apply<F>(mut slf: PyRefMut<Self>, fun: F) -> PyResult<PyRefMut<Self>>
    where
        F: FnOnce(reqwest::multipart::Part) -> PyResult<reqwest::multipart::Part>,
    {
        let builder = slf
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Part was already consumed"))?;
        slf.inner = Some(fun(builder)?);
        Ok(slf)
    }

    pub fn take_inner(&mut self) -> PyResult<reqwest::multipart::Part> {
        self.inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Part was already consumed"))
    }
}
impl From<reqwest::multipart::Part> for Part {
    fn from(part: reqwest::multipart::Part) -> Self {
        Part { inner: Some(part) }
    }
}
