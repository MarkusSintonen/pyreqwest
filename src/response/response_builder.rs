use crate::http::Body;
use crate::http::{Extensions, HeaderMap, StatusCode, Version};
use crate::response::{ConsumeBodyConfig, Response};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

#[pyclass]
#[derive(Default)]
pub struct ResponseBuilder {
    inner: Option<http::response::Builder>,
    body: Option<Body>,
    extensions: Option<Extensions>,
}
#[pymethods]
impl ResponseBuilder {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: Some(http::response::Builder::new()),
            ..Default::default()
        }
    }

    pub async fn build(&mut self) -> PyResult<Response> {
        let body: reqwest::Body = self
            .body
            .take()
            .map(|b| b.try_into())
            .transpose()?
            .unwrap_or_else(|| reqwest::Body::from(Vec::new()));
        let mut resp = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Response was already built"))?
            .body(body)
            .map_err(|e| PyValueError::new_err(format!("Failed to build response: {}", e)))?;
        self.extensions.take().map(|ext| resp.extensions_mut().insert(ext));
        let resp = reqwest::Response::from(resp);
        Response::initialize(resp, None, ConsumeBodyConfig::Fully).await
    }

    pub fn status(slf: PyRefMut<Self>, value: StatusCode) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.status(value.0)))
    }

    pub fn version(slf: PyRefMut<Self>, value: Version) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.version(value.0)))
    }

    pub fn header(slf: PyRefMut<Self>, key: String, value: String) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.header(key, value)))
    }

    pub fn headers(slf: PyRefMut<Self>, headers: HeaderMap) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |mut builder| {
            let headers_mut = builder
                .headers_mut()
                .ok_or_else(|| PyRuntimeError::new_err("ResponseBuilder has an error"))?;
            *headers_mut = headers.0;
            Ok(builder)
        })
    }

    pub fn body(&mut self, value: Bound<PyAny>) -> PyResult<()> {
        if value.is_none() {
            self.body = None;
        } else {
            self.body = Some(value.downcast::<Body>()?.try_borrow()?.try_clone()?);
        }
        Ok(())
    }

    pub fn extensions(mut slf: PyRefMut<Self>, value: Option<Extensions>) -> PyRefMut<Self> {
        slf.extensions = value;
        slf
    }
}
impl ResponseBuilder {
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
