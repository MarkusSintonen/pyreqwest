use crate::allow_threads::AllowThreads;
use crate::client::RuntimeHandle;
use crate::http::internal::types::{Extensions, HeaderName, HeaderValue, JsonValue, StatusCode, Version};
use crate::http::{HeaderMap, RequestBody};
use crate::response::internal::{BodyConsumeConfig, StreamedReadConfig};
use crate::response::{BaseResponse, Response, SyncResponse};
use bytes::Bytes;
use http::header::CONTENT_TYPE;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::{PyTraverseError, PyVisit};
use pyo3_bytes::PyBytes;

#[pyclass]
pub struct ResponseBuilder {
    head: Option<http::response::Parts>,
    body: Option<RequestBody>,
    extensions: Option<Extensions>,
}

#[pymethods]
impl ResponseBuilder {
    #[new]
    fn new_py() -> Self {
        Self::new()
    }

    fn status(mut slf: PyRefMut<Self>, status: StatusCode) -> PyResult<PyRefMut<Self>> {
        let head = slf.mut_head()?;
        head.status = status.0;
        Ok(slf)
    }

    fn version(mut slf: PyRefMut<Self>, version: Version) -> PyResult<PyRefMut<Self>> {
        let head = slf.mut_head()?;
        head.version = version.0;
        Ok(slf)
    }

    fn header(mut slf: PyRefMut<Self>, name: HeaderName, value: HeaderValue) -> PyResult<PyRefMut<Self>> {
        let head = slf.mut_head()?;
        head.headers
            .try_append(name.0, value.0)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(slf)
    }

    fn headers(mut slf: PyRefMut<'_, Self>, headers: HeaderMap) -> PyResult<PyRefMut<'_, Self>> {
        let head = slf.mut_head()?;
        headers.extend_into_inner(&mut head.headers)?;
        Ok(slf)
    }

    fn extensions(mut slf: PyRefMut<Self>, value: Extensions) -> PyRefMut<Self> {
        slf.extensions = Some(value);
        slf
    }

    fn body_bytes(mut slf: PyRefMut<Self>, body: PyBytes) -> PyResult<PyRefMut<Self>> {
        slf.body = Some(RequestBody::from_bytes(body));
        Ok(slf)
    }

    fn body_text(mut slf: PyRefMut<Self>, body: String) -> PyResult<PyRefMut<Self>> {
        slf.body = Some(RequestBody::from_text(body));
        Ok(slf)
    }

    fn body_json<'py>(mut slf: PyRefMut<'py, Self>, data: JsonValue, py: Python<'py>) -> PyResult<PyRefMut<'py, Self>> {
        let bytes = py.detach(|| serde_json::to_vec(&data).map_err(|e| PyValueError::new_err(e.to_string())))?;
        slf.body = Some(RequestBody::from(Bytes::from(bytes)));
        slf.mut_head()?
            .headers
            .try_append(CONTENT_TYPE, "application/json".parse().unwrap())
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(slf)
    }

    fn body_stream<'py>(mut slf: PyRefMut<'py, Self>, stream: Bound<'py, PyAny>) -> PyResult<PyRefMut<'py, Self>> {
        slf.body = Some(RequestBody::from_stream(stream)?);
        Ok(slf)
    }

    async fn build(slf: Py<Self>) -> PyResult<Py<Response>> {
        let inner = Python::attach(|py| slf.bind(py).try_borrow_mut()?.build_inner(false))?;

        let config = BodyConsumeConfig::Streamed(StreamedReadConfig::default());
        let runtime = RuntimeHandle::global_handle()?.clone();
        let resp = AllowThreads(BaseResponse::initialize(inner, None, config, runtime, None, false)).await?;

        Python::attach(|py| Response::new_py(py, resp))
    }

    fn build_sync(mut slf: PyRefMut<Self>) -> PyResult<Py<SyncResponse>> {
        let inner = slf.build_inner(true)?;

        let config = BodyConsumeConfig::Streamed(StreamedReadConfig::default());
        let runtime = RuntimeHandle::global_handle()?;
        let resp =
            runtime.blocking_spawn(BaseResponse::initialize(inner, None, config, runtime.clone(), None, false))?;

        Python::attach(|py| SyncResponse::new_py(py, resp))
    }

    fn copy(&self) -> PyResult<Self> {
        self.__copy__()
    }

    fn __copy__(&self) -> PyResult<Self> {
        Ok(Self {
            head: Some(
                self.head
                    .as_ref()
                    .ok_or_else(|| PyRuntimeError::new_err("Response was already built"))?
                    .clone(),
            ),
            body: self.body.as_ref().map(|b| b.try_clone()).transpose()?,
            extensions: self.extensions.as_ref().map(|e| e.copy()).transpose()?,
        })
    }

    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Some(ext) = &self.extensions {
            visit.call(&ext.0)?;
        }
        if let Some(body) = &self.body {
            body.__traverse__(visit)?;
        }
        Ok(())
    }

    fn __clear__(&mut self) {
        self.body = None;
        self.extensions = None;
    }
}

impl Default for ResponseBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponseBuilder {
    pub fn new() -> Self {
        let (head, _) = http::response::Response::new(()).into_parts();
        Self {
            head: Some(head),
            body: None,
            extensions: None,
        }
    }

    fn build_inner(&mut self, is_blocking: bool) -> PyResult<reqwest::Response> {
        let body: reqwest::Body = self
            .body
            .take()
            .map(|b| {
                b.set_task_local()?;
                b.into_reqwest(is_blocking)
            })
            .transpose()?
            .unwrap_or_else(|| reqwest::Body::from(b"".as_ref()));

        let mut head = self
            .head
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Response was already built"))?;

        self.extensions.take().map(|ext| head.extensions.insert(ext));

        Ok(reqwest::Response::from(http::response::Response::from_parts(head, body)))
    }

    fn mut_head(&mut self) -> PyResult<&mut http::response::Parts> {
        self.head
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("Response was already built"))
    }
}
