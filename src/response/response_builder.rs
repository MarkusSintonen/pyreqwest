use crate::allow_threads::AllowThreads;
use crate::client::Handle;
use crate::http::{Extensions, StatusCode, Version};
use crate::http::{HeaderMap, HeaderName, HeaderValue, JsonValue, RequestBody};
use crate::response::{
    BaseResponse, BlockingResponse, BodyConsumeConfig, DEFAULT_READ_BUFFER_LIMIT, Response, StreamedReadConfig,
};
use bytes::Bytes;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::{PyTraverseError, PyVisit};
use pyo3_bytes::PyBytes;

#[pyclass]
pub struct ResponseBuilder {
    inner: Option<http::response::Builder>,
    body: Option<RequestBody>,
    extensions: Option<Extensions>,
}

#[pymethods]
impl ResponseBuilder {
    #[new]
    fn new_py() -> Self {
        Self::new()
    }

    fn status(slf: PyRefMut<Self>, value: StatusCode) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.status(value.0)))
    }

    fn version(slf: PyRefMut<Self>, value: Version) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.version(value.0)))
    }

    fn header(slf: PyRefMut<Self>, name: HeaderName, value: HeaderValue) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.header(name.0, value.0)))
    }

    fn headers(slf: PyRefMut<'_, Self>, headers: HeaderMap) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |mut builder| {
            let headers_mut = builder
                .headers_mut()
                .ok_or_else(|| PyRuntimeError::new_err("ResponseBuilder has an error"))?;
            headers.extend_into_inner(headers_mut)?;
            Ok(builder)
        })
    }

    fn extensions(mut slf: PyRefMut<Self>, value: Extensions) -> PyRefMut<Self> {
        slf.extensions = Some(value);
        slf
    }

    fn body(mut slf: PyRefMut<'_, Self>, body: Option<Py<RequestBody>>) -> PyResult<PyRefMut<'_, Self>> {
        slf.body = body.map(|v| v.get().take_inner()).transpose()?;
        Ok(slf)
    }

    fn body_bytes(mut slf: PyRefMut<Self>, body: PyBytes) -> PyResult<PyRefMut<Self>> {
        slf.body = Some(RequestBody::from_bytes(body));
        Ok(slf)
    }

    fn body_text(mut slf: PyRefMut<Self>, body: String) -> PyResult<PyRefMut<Self>> {
        slf.body = Some(RequestBody::from_text(body));
        Ok(slf)
    }

    fn body_json(mut slf: PyRefMut<'_, Self>, data: JsonValue) -> PyResult<PyRefMut<'_, Self>> {
        let bytes = slf
            .py()
            .detach(|| serde_json::to_vec(&data).map_err(|e| PyValueError::new_err(e.to_string())))?;
        slf.body = Some(RequestBody::from(Bytes::from(bytes)));
        Self::apply(slf, |builder| Ok(builder.header("content-type", "application/json")))
    }

    fn body_stream<'py>(mut slf: PyRefMut<'py, Self>, stream: Bound<'py, PyAny>) -> PyResult<PyRefMut<'py, Self>> {
        slf.body = Some(RequestBody::from_stream(stream)?);
        Ok(slf)
    }

    async fn build(slf: Py<Self>) -> PyResult<Py<Response>> {
        let inner = Python::attach(|py| slf.bind(py).try_borrow_mut()?.build_inner())?;

        let config = BodyConsumeConfig::Streamed(StreamedReadConfig {
            read_buffer_limit: DEFAULT_READ_BUFFER_LIMIT,
        });
        let resp = AllowThreads(BaseResponse::initialize(inner, None, config, None)).await?;

        Python::attach(|py| Response::new_py(py, resp))
    }

    fn build_blocking(mut slf: PyRefMut<Self>) -> PyResult<Py<BlockingResponse>> {
        let inner = slf.build_inner()?;

        let config = BodyConsumeConfig::Streamed(StreamedReadConfig {
            read_buffer_limit: DEFAULT_READ_BUFFER_LIMIT,
        });
        let resp = Handle::global_handle()?.blocking_spawn(BaseResponse::initialize(inner, None, config, None))?;

        Python::attach(|py| BlockingResponse::new_py(py, resp))
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
        Self {
            inner: Some(http::response::Builder::new()),
            body: None,
            extensions: None,
        }
    }

    fn build_inner(&mut self) -> PyResult<reqwest::Response> {
        let body: reqwest::Body = self
            .body
            .take()
            .map(|b| {
                b.set_task_local()?;
                b.into_reqwest()
            })
            .transpose()?
            .unwrap_or_else(|| reqwest::Body::from(b"".as_ref()));

        let mut resp = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Response was already built"))?
            .body(body)
            .map_err(|e| PyValueError::new_err(format!("Failed to build response: {}", e)))?;

        self.extensions.take().map(|ext| resp.extensions_mut().insert(ext));

        Ok(reqwest::Response::from(resp))
    }

    fn apply<F>(mut slf: PyRefMut<Self>, fun: F) -> PyResult<PyRefMut<Self>>
    where
        F: FnOnce(http::response::Builder) -> PyResult<http::response::Builder>,
        F: Send,
    {
        let builder = slf
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Response was already built"))?;
        slf.inner = Some(slf.py().detach(|| fun(builder))?);
        Ok(slf)
    }
}
