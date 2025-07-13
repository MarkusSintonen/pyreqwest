use crate::client::runtime::Runtime;
use crate::http_types::{MethodExt, UrlExt};
use crate::request::RequestBuilder;
use pyo3::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use http::Method;
use tokio::sync::Semaphore;

#[pyclass]
pub struct Client {
    client: reqwest::Client,
    runtime: Arc<Runtime>,
    middlewares: Option<Arc<Vec<Py<PyAny>>>>,
    request_semaphore: Option<Arc<Semaphore>>,
    connect_timeout: Option<Duration>,
}

#[pymethods]
impl Client {
    fn request(&self, method: MethodExt, url: UrlExt) -> PyResult<RequestBuilder> {
        let runtime = self.runtime.clone();
        let middlewares = self.middlewares.clone();
        let request_semaphore = self.request_semaphore.clone();
        let connect_timeout = self.connect_timeout.clone();

        let url: reqwest::Url = url.try_into()?;
        let request = self.client.request(method.0, url);
        Ok(RequestBuilder::new(runtime, request, middlewares, request_semaphore, connect_timeout))
    }

    pub fn get(&self, url: UrlExt) -> PyResult<RequestBuilder> {
        self.request(Method::GET.into(), url)
    }

    pub fn post(&self, url: UrlExt) -> PyResult<RequestBuilder> {
        self.request(Method::POST.into(), url)
    }

    pub fn put(&self, url: UrlExt) -> PyResult<RequestBuilder> {
        self.request(Method::PUT.into(), url)
    }

    pub fn patch(&self, url: UrlExt) -> PyResult<RequestBuilder> {
        self.request(Method::PATCH.into(), url)
    }

    pub fn delete(&self, url: UrlExt) -> PyResult<RequestBuilder> {
        self.request(Method::DELETE.into(), url)
    }

    pub fn head(&self, url: UrlExt) -> PyResult<RequestBuilder> {
        self.request(Method::HEAD.into(), url)
    }

    async fn __aenter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    async fn __aexit__(&self, _exc_type: Py<PyAny>, _exc_val: Py<PyAny>, _traceback: Py<PyAny>) {
        self.close().await;
    }

    async fn close(&self) {
        self.runtime.close().await;
    }
}
impl Client {
    pub fn new(
        client: reqwest::Client,
        runtime: Runtime,
        middlewares: Option<Vec<Py<PyAny>>>,
        max_connections: Option<usize>,
        connect_timeout: Option<Duration>,
    ) -> Self {
        Client {
            client,
            runtime: Arc::new(runtime),
            request_semaphore: max_connections.map(|limit| Arc::new(Semaphore::new(limit))),
            middlewares: middlewares.map(Arc::new),
            connect_timeout,
        }
    }
}
