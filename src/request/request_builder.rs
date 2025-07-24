use crate::client::runtime::Runtime;
use crate::http::Body;
use crate::http::{Extensions, HeaderMap, JsonValue, Version};
use crate::multipart::Form;
use crate::request::Request;
use crate::request::connection_limiter::ConnectionLimiter;
use crate::request::consumed_request::ConsumedRequest;
use crate::request::stream_request::StreamRequest;
use crate::response::ConsumeBodyConfig;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3_bytes::PyBytes;
use std::sync::Arc;
use std::time::Duration;

#[pyclass]
pub struct RequestBuilder {
    runtime: Arc<Runtime>,
    inner: Option<reqwest::RequestBuilder>,
    body: Option<Body>,
    extensions: Option<Extensions>,
    middlewares: Option<Arc<Vec<Py<PyAny>>>>,
    connection_limiter: Option<ConnectionLimiter>,
    error_for_status: bool,
}
#[pymethods]
impl RequestBuilder {
    fn build_consumed(&mut self) -> PyResult<Py<ConsumedRequest>> {
        ConsumedRequest::new_py(self.inner_build(ConsumeBodyConfig::Fully)?)
    }

    fn build_streamed(&mut self) -> PyResult<Py<StreamRequest>> {
        let init_read = StreamRequest::default_initial_read_size();
        StreamRequest::new_py(self.inner_build(ConsumeBodyConfig::Partially(init_read))?)
    }

    fn error_for_status(mut slf: PyRefMut<Self>, value: bool) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.error_for_status = value;
        Ok(slf)
    }

    fn header(slf: PyRefMut<Self>, key: String, value: String) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.header(key, value)))
    }

    fn headers(slf: PyRefMut<Self>, headers: HeaderMap) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.headers(headers.0)))
    }

    fn basic_auth(slf: PyRefMut<Self>, username: String, password: Option<String>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.basic_auth(username, password)))
    }

    fn bearer_auth(slf: PyRefMut<Self>, token: String) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.bearer_auth(token)))
    }

    fn body_bytes(mut slf: PyRefMut<Self>, body: PyBytes) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.body = Some(Body::from_bytes(body));
        Ok(slf)
    }

    fn body_text(mut slf: PyRefMut<Self>, body: String) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.body = Some(Body::from_str(body));
        Ok(slf)
    }

    fn body_stream(mut slf: PyRefMut<Self>, async_gen: Py<PyAny>) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.body = Some(Body::from_stream(slf.py(), async_gen));
        Ok(slf)
    }

    fn timeout(slf: PyRefMut<Self>, timeout: Duration) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.timeout(timeout)))
    }

    fn multipart<'py>(slf: PyRefMut<'py, Self>, multipart: Bound<'py, Form>) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| Ok(builder.multipart(multipart.try_borrow_mut()?.build()?)))
    }

    fn query(slf: PyRefMut<Self>, query: JsonValue) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.query(&query.0)))
    }

    fn version(slf: PyRefMut<Self>, version: Version) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.version(version.0)))
    }

    fn form(slf: PyRefMut<Self>, form: JsonValue) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.form(&form.0)))
    }

    fn extensions(mut slf: PyRefMut<Self>, extensions: Extensions) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.extensions = Some(extensions);
        Ok(slf)
    }
}
impl RequestBuilder {
    pub fn new(
        runtime: Arc<Runtime>,
        inner: reqwest::RequestBuilder,
        middlewares: Option<Arc<Vec<Py<PyAny>>>>,
        connection_limiter: Option<ConnectionLimiter>,
        error_for_status: bool,
    ) -> Self {
        RequestBuilder {
            runtime,
            inner: Some(inner),
            body: None,
            extensions: None,
            middlewares,
            connection_limiter,
            error_for_status,
        }
    }

    fn inner_build(&mut self, consume_body: ConsumeBodyConfig) -> PyResult<Request> {
        let (client, request) = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))?
            .build_split();
        let request = request.map_err(|e| PyValueError::new_err(format!("Failed to build request: {}", e)))?;
        let request = Request::new(
            self.runtime.clone(),
            client,
            request,
            self.body.take(),
            self.extensions.take(),
            self.middlewares.clone(),
            self.connection_limiter.clone(),
            self.error_for_status,
            consume_body,
        );
        Ok(request)
    }

    pub fn inner_timeout(&mut self, timeout: Duration) -> PyResult<&mut RequestBuilder> {
        self.apply_inner(|b| Ok(b.timeout(timeout)))
    }

    pub fn inner_headers(&mut self, headers: HeaderMap) -> PyResult<&mut RequestBuilder> {
        self.apply_inner(|b| Ok(b.headers(headers.0)))
    }

    fn check_inner(&self) -> PyResult<()> {
        self.inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))
            .map(|_| ())
    }

    fn apply<F>(mut slf: PyRefMut<Self>, fun: F) -> PyResult<PyRefMut<Self>>
    where
        F: FnOnce(reqwest::RequestBuilder) -> PyResult<reqwest::RequestBuilder>,
    {
        let builder = slf
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))?;
        slf.inner = Some(fun(builder)?);
        Ok(slf)
    }

    fn apply_inner<F>(&mut self, fun: F) -> PyResult<&mut RequestBuilder>
    where
        F: FnOnce(reqwest::RequestBuilder) -> PyResult<reqwest::RequestBuilder>,
    {
        let builder = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))?;
        self.inner = Some(fun(builder)?);
        Ok(self)
    }
}
