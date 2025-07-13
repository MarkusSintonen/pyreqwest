use crate::client::runtime::Runtime;
use crate::http::types::{Extensions, HeaderMap, JsonValue, Version};
use crate::multipart::form::Form;
use crate::request::Request;
use crate::request::RequestBody;
use crate::request::RequestWrapper;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3_bytes::PyBytes;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

#[pyclass]
pub struct RequestBuilder {
    runtime: Arc<Runtime>,
    inner: Option<reqwest::RequestBuilder>,
    body: Option<RequestBody>,
    middlewares: Option<Arc<Vec<Py<PyAny>>>>,
    request_semaphore: Option<Arc<Semaphore>>,
    connect_timeout: Option<Duration>,
    extensions: Option<Extensions>,
}
#[pymethods]
impl RequestBuilder {
    fn build(&mut self) -> PyResult<Request> {
        let (client, request) = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))?
            .build_split();
        let request = request.map_err(|e| PyValueError::new_err(format!("Failed to build request: {}", e)))?;
        let request = Request::new(
            self.runtime.clone(),
            client,
            RequestWrapper::new(request, self.body.take(), self.extensions.take()),
            self.middlewares.clone(),
            self.request_semaphore.clone(),
            self.connect_timeout.clone(),
        );
        Ok(request)
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
        if slf.inner.is_none() {
            return Err(PyRuntimeError::new_err("Request was already built"));
        }
        slf.body = Some(RequestBody::from_bytes(body));
        Ok(slf)
    }

    fn body_str(mut slf: PyRefMut<Self>, body: String) -> PyResult<PyRefMut<Self>> {
        if slf.inner.is_none() {
            return Err(PyRuntimeError::new_err("Request was already built"));
        }
        slf.body = Some(RequestBody::from_str(body));
        Ok(slf)
    }

    fn body_stream(mut slf: PyRefMut<Self>, async_gen: Py<PyAny>) -> PyResult<PyRefMut<Self>> {
        if slf.inner.is_none() {
            return Err(PyRuntimeError::new_err("Request was already built"));
        }
        slf.body = Some(RequestBody::from_stream(async_gen));
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
        if slf.inner.is_none() {
            return Err(PyRuntimeError::new_err("Request was already built"));
        }
        slf.extensions = Some(extensions);
        Ok(slf)
    }
}
impl RequestBuilder {
    pub fn new(
        runtime: Arc<Runtime>,
        inner: reqwest::RequestBuilder,
        middlewares: Option<Arc<Vec<Py<PyAny>>>>,
        request_semaphore: Option<Arc<Semaphore>>,
        connect_timeout: Option<Duration>,
    ) -> Self {
        RequestBuilder {
            runtime,
            inner: Some(inner),
            body: None,
            middlewares,
            request_semaphore,
            connect_timeout,
            extensions: None,
        }
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
}
