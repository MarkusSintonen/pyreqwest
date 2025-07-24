use crate::client::runtime::Runtime;
use crate::http::UrlType;
use crate::http::{HeaderMap, Method};
use crate::request::{ConnectionLimiter, RequestBuilder};
use pyo3::prelude::*;
use std::sync::Arc;
use std::time::Duration;

#[pyclass]
pub struct Client {
    client: reqwest::Client,
    runtime: Arc<Runtime>,
    middlewares: Option<Arc<Vec<Py<PyAny>>>>,
    total_timeout: Option<Duration>,
    connection_limiter: Option<ConnectionLimiter>,
    error_for_status: bool,
    default_headers: Option<HeaderMap>,
}

#[pymethods]
impl Client {
    fn request(&self, method: Method, url: UrlType) -> PyResult<RequestBuilder> {
        let runtime = self.runtime.clone();
        let middlewares = self.middlewares.clone();
        let connection_limiter = self.connection_limiter.clone();

        let request = self.client.request(method.0, url.0);
        let mut builder = RequestBuilder::new(runtime, request, middlewares, connection_limiter, self.error_for_status);
        self.total_timeout
            .map(|timeout| builder.inner_timeout(timeout))
            .transpose()?;
        self.default_headers
            .clone()
            .map(|default_headers| builder.inner_headers(default_headers))
            .transpose()?;
        Ok(builder)
    }

    pub fn get(&self, url: UrlType) -> PyResult<RequestBuilder> {
        self.request(http::Method::GET.into(), url)
    }

    pub fn post(&self, url: UrlType) -> PyResult<RequestBuilder> {
        self.request(http::Method::POST.into(), url)
    }

    pub fn put(&self, url: UrlType) -> PyResult<RequestBuilder> {
        self.request(http::Method::PUT.into(), url)
    }

    pub fn patch(&self, url: UrlType) -> PyResult<RequestBuilder> {
        self.request(http::Method::PATCH.into(), url)
    }

    pub fn delete(&self, url: UrlType) -> PyResult<RequestBuilder> {
        self.request(http::Method::DELETE.into(), url)
    }

    pub fn head(&self, url: UrlType) -> PyResult<RequestBuilder> {
        self.request(http::Method::HEAD.into(), url)
    }

    async fn __aenter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    async fn __aexit__(&self, _exc_type: Py<PyAny>, _exc_val: Py<PyAny>, _traceback: Py<PyAny>) {
        self.close().await;
    }

    async fn close(&self) {
        self.runtime.close();
    }
}
impl Client {
    pub fn new(
        client: reqwest::Client,
        runtime: Runtime,
        middlewares: Option<Vec<Py<PyAny>>>,
        total_timeout: Option<Duration>,
        connection_limiter: Option<ConnectionLimiter>,
        error_for_status: bool,
        default_headers: Option<HeaderMap>,
    ) -> Self {
        Client {
            client,
            runtime: Arc::new(runtime),
            middlewares: middlewares.map(Arc::new),
            total_timeout,
            connection_limiter,
            error_for_status,
            default_headers,
        }
    }
}
