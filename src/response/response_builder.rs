use crate::client::Client;
use crate::http::{Body, HeaderMap, HeaderName, HeaderValue, JsonValue};
use crate::http::{Extensions, StatusCode, Version};
use crate::response::{BodyConsumeConfig, Response};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3_bytes::PyBytes;

#[pyclass]
pub struct ResponseBuilder {
    client: Option<Client>,
    inner: Option<http::response::Builder>,
    body: Option<Body>,
    extensions: Option<Extensions>,
}
#[pymethods]
impl ResponseBuilder {
    async fn build(&mut self) -> PyResult<Response> {
        let body: reqwest::Body = self
            .body
            .take()
            .map(|mut b| {
                Python::with_gil(|py| b.set_task_local(py, self.client.as_ref()))?;
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

        let resp = reqwest::Response::from(resp);
        Response::initialize(resp, None, BodyConsumeConfig::Fully).await
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

    #[staticmethod]
    pub fn create_for_mocking() -> PyResult<Self> {
        Ok(ResponseBuilder {
            client: None,
            inner: Some(http::response::Builder::new()),
            body: None,
            extensions: None,
        })
    }
}
impl ResponseBuilder {
    pub fn new(client: Client) -> Self {
        Self {
            client: Some(client),
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
