use crate::client::Client;
use crate::http::{Body, ExtensionsType, HeaderName, HeaderValue, HeadersType};
use crate::http::{Extensions, StatusCode, Version};
use crate::response::{BodyConsumeConfig, Response};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

#[pyclass]
pub struct ResponseBuilder {
    client: Client,
    inner: Option<http::response::Builder>,
    body: Option<Body>,
    extensions: Option<Extensions>,
}
#[pymethods]
impl ResponseBuilder {
    pub async fn build(&mut self) -> PyResult<Response> {
        let body: reqwest::Body = self
            .body
            .take()
            .map(|mut b| {
                Python::with_gil(|py| b.set_task_local(py, Some(&self.client)))?;
                b.to_reqwest()
            })
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
        Response::initialize(resp, None, BodyConsumeConfig::Fully).await
    }

    pub fn status(slf: PyRefMut<Self>, value: StatusCode) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.status(value.0)))
    }

    pub fn version(slf: PyRefMut<Self>, value: Version) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.version(value.0)))
    }

    pub fn header(slf: PyRefMut<Self>, name: HeaderName, value: HeaderValue) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.header(name.0, value.0)))
    }

    pub fn headers<'py>(slf: PyRefMut<'py, Self>, mut headers: HeadersType) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |mut builder| {
            let headers_mut = builder
                .headers_mut()
                .ok_or_else(|| PyRuntimeError::new_err("ResponseBuilder has an error"))?;
            *headers_mut = headers.0.try_take_inner()?;
            Ok(builder)
        })
    }

    pub fn body<'py>(mut slf: PyRefMut<'py, Self>, value: Bound<PyAny>) -> PyResult<PyRefMut<'py, Self>> {
        if value.is_none() {
            slf.body = None;
        } else {
            slf.body = Some(value.downcast::<Body>()?.try_borrow_mut()?.take_inner()?);
        }
        Ok(slf)
    }

    pub fn extensions(mut slf: PyRefMut<Self>, value: ExtensionsType) -> PyRefMut<Self> {
        slf.extensions = Some(value.0);
        slf
    }
}
impl ResponseBuilder {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            inner: Some(http::response::Builder::new()),
            body: None,
            extensions: None,
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
