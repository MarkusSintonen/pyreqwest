use crate::client::Client;
use crate::exceptions::BuilderError;
use crate::http::{Body, Extensions, FormParams, HeaderMap, HeaderName, HeaderValue, JsonValue, QueryParams};
use crate::multipart::Form;
use crate::request::Request;
use crate::request::consumed_request::ConsumedRequest;
use crate::request::stream_request::StreamRequest;
use crate::response::BodyConsumeConfig;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3_bytes::PyBytes;
use std::time::Duration;

#[pyclass]
pub struct RequestBuilder {
    client: Client,
    inner: Option<reqwest::RequestBuilder>,
    body: Option<Body>,
    extensions: Option<Extensions>,
    error_for_status: bool,
}
#[pymethods]
impl RequestBuilder {
    fn build_consumed(&mut self) -> PyResult<Py<ConsumedRequest>> {
        ConsumedRequest::new_py(self.inner_build(BodyConsumeConfig::Fully)?)
    }

    fn build_streamed(&mut self) -> PyResult<Py<StreamRequest>> {
        let init_read = StreamRequest::default_initial_read_size();
        StreamRequest::new_py(self.inner_build(BodyConsumeConfig::Partially(init_read))?)
    }

    fn error_for_status(mut slf: PyRefMut<Self>, value: bool) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.error_for_status = value;
        Ok(slf)
    }

    fn header(slf: PyRefMut<Self>, name: HeaderName, value: HeaderValue) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.header(name.0, value.0)))
    }

    fn headers<'py>(slf: PyRefMut<'py, Self>, mut headers: HeaderMap) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| Ok(builder.headers(headers.try_take_inner()?)))
    }

    fn basic_auth(slf: PyRefMut<Self>, username: String, password: Option<String>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.basic_auth(username, password)))
    }

    fn bearer_auth(slf: PyRefMut<Self>, token: String) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.bearer_auth(token)))
    }

    fn body<'py>(mut slf: PyRefMut<'py, Self>, body: Option<Bound<Body>>) -> PyResult<PyRefMut<'py, Self>> {
        slf.check_inner()?;
        slf.body = body
            .map(|v| Ok::<_, PyErr>(v.try_borrow_mut()?.take_inner()?))
            .transpose()?;
        Ok(slf)
    }

    fn body_bytes(mut slf: PyRefMut<Self>, body: PyBytes) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.body = Some(Body::from_bytes(body));
        Ok(slf)
    }

    fn body_text(mut slf: PyRefMut<Self>, body: String) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.body = Some(Body::from_text(body));
        Ok(slf)
    }

    fn body_json<'py>(mut slf: PyRefMut<'py, Self>, data: JsonValue) -> PyResult<PyRefMut<'py, Self>> {
        slf.check_inner()?;
        let bytes = serde_json::to_vec(&data).map_err(|e| PyValueError::new_err(e.to_string()))?;
        slf.body = Some(bytes.into());
        Self::apply(slf, |builder| Ok(builder.header("content-type", "application/json")))
    }

    fn body_stream(mut slf: PyRefMut<Self>, stream: Py<PyAny>) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.body = Some(Body::from_stream(slf.py(), stream)?);
        Ok(slf)
    }

    fn timeout(slf: PyRefMut<Self>, timeout: Duration) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.timeout(timeout)))
    }

    fn multipart<'py>(slf: PyRefMut<'py, Self>, multipart: Bound<'_, Form>) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| Ok(builder.multipart(multipart.try_borrow_mut()?.build()?)))
    }

    fn query<'py>(slf: PyRefMut<'py, Self>, query: Bound<'_, PyAny>) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| Ok(builder.query(&query.extract::<QueryParams>()?.0)))
    }

    fn form<'py>(slf: PyRefMut<'py, Self>, form: Bound<'_, PyAny>) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| Ok(builder.form(&form.extract::<FormParams>()?.0)))
    }

    fn extensions<'py>(mut slf: PyRefMut<'py, Self>, extensions: Extensions) -> PyResult<PyRefMut<'py, Self>> {
        slf.check_inner()?;
        slf.extensions = Some(extensions);
        Ok(slf)
    }
}
impl RequestBuilder {
    pub fn new(client: Client, inner: reqwest::RequestBuilder, error_for_status: bool) -> Self {
        RequestBuilder {
            client,
            inner: Some(inner),
            body: None,
            extensions: None,
            error_for_status,
        }
    }

    fn inner_build(&mut self, consume_body: BodyConsumeConfig) -> PyResult<Request> {
        let request = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))?
            .build()
            .map_err(|e| BuilderError::from_err("Failed to build request", &e))?;

        if request.body().is_some() && self.body.is_some() {
            return Err(BuilderError::from_causes("Can not set body when multipart or form is used", vec![]));
        }

        let request = Request::new(
            self.client.clone(),
            request,
            self.body.take(),
            self.extensions.take(),
            self.error_for_status,
            consume_body,
        );
        Ok(request)
    }

    pub fn inner_timeout(&mut self, timeout: Duration) -> PyResult<&mut RequestBuilder> {
        self.apply_inner(|b| Ok(b.timeout(timeout)))
    }

    pub fn inner_headers(&mut self, headers: &HeaderMap) -> PyResult<&mut RequestBuilder> {
        self.apply_inner(|b| Ok(b.headers(headers.try_clone_inner()?)))
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
