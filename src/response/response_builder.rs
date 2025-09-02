use crate::http::{Body, HeaderMap, HeaderName, HeaderValue, JsonValue};
use crate::http::{Extensions, StatusCode, Version};
use crate::response::{BodyConsumeConfig, DEFAULT_READ_BUFFER_LIMIT, Response, StreamedReadConfig};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::{PyTraverseError, PyVisit};
use pyo3_bytes::PyBytes;

#[pyclass]
pub struct ResponseBuilder {
    inner: Option<http::response::Builder>,
    body: Option<Body>,
    extensions: Option<Extensions>,
    streamed_read_buffer_limit: Option<usize>,
}
#[pymethods]
impl ResponseBuilder {
    #[new]
    fn py_new() -> Self {
        Self::new()
    }

    async fn build(&mut self) -> PyResult<Response> {
        let body: reqwest::Body = self
            .body
            .take()
            .map(|mut b| {
                Python::with_gil(|py| b.set_task_local(py))?;
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

        let inner_resp = reqwest::Response::from(resp);

        let config = StreamedReadConfig {
            read_buffer_limit: self.streamed_read_buffer_limit.unwrap_or(DEFAULT_READ_BUFFER_LIMIT),
        };

        Response::initialize(inner_resp, None, BodyConsumeConfig::Streamed(config)).await
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

    fn body<'py>(mut slf: PyRefMut<'py, Self>, body: Option<Bound<Body>>) -> PyResult<PyRefMut<'py, Self>> {
        slf.body = body.map(|v| v.try_borrow_mut()?.take_inner()).transpose()?;
        Ok(slf)
    }

    fn body_bytes(mut slf: PyRefMut<Self>, body: PyBytes) -> PyResult<PyRefMut<Self>> {
        slf.body = Some(Body::from_bytes(body));
        Ok(slf)
    }

    fn body_text(mut slf: PyRefMut<Self>, body: String) -> PyResult<PyRefMut<Self>> {
        slf.body = Some(Body::from_text(body));
        Ok(slf)
    }

    fn body_json(mut slf: PyRefMut<'_, Self>, data: JsonValue) -> PyResult<PyRefMut<'_, Self>> {
        let bytes = serde_json::to_vec(&data).map_err(|e| PyValueError::new_err(e.to_string()))?;
        slf.body = Some(bytes.into());
        Self::apply(slf, |builder| Ok(builder.header("content-type", "application/json")))
    }

    fn body_stream(mut slf: PyRefMut<Self>, stream: Py<PyAny>) -> PyResult<PyRefMut<Self>> {
        slf.body = Some(Body::from_stream(slf.py(), stream)?);
        Ok(slf)
    }

    fn streamed_read_buffer_limit(mut slf: PyRefMut<'_, Self>, value: usize) -> PyResult<PyRefMut<'_, Self>> {
        slf.streamed_read_buffer_limit = Some(value);
        Ok(slf)
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
            streamed_read_buffer_limit: None,
        }
    }

    fn apply<F>(mut slf: PyRefMut<Self>, fun: F) -> PyResult<PyRefMut<Self>>
    where
        F: FnOnce(http::response::Builder) -> PyResult<http::response::Builder>,
    {
        let builder = slf
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Response was already built"))?;
        slf.inner = Some(fun(builder)?);
        Ok(slf)
    }
}
