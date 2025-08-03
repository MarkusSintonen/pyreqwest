use crate::asyncio::EventLoopCell;
use crate::client::runtime::Runtime;
use crate::exceptions::PoolTimeoutError;
use crate::exceptions::utils::map_send_error;
use crate::http::UrlType;
use crate::http::{HeaderMap, Method};
use crate::middleware::Next;
use crate::request::{ConnectionLimiter, RequestBuilder};
use pyo3::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OwnedSemaphorePermit;

#[pyclass]
pub struct Client {
    inner: Arc<ClientInner>,
}
struct ClientInner {
    client: reqwest::Client,
    runtime: Runtime,
    middlewares: Option<Vec<Py<PyAny>>>,
    total_timeout: Option<Duration>,
    connection_limiter: Option<ConnectionLimiter>,
    error_for_status: bool,
    default_headers: Option<HeaderMap>,
    event_loop_cell: EventLoopCell,
}

#[pymethods]
impl Client {
    fn request(&self, method: Method, url: UrlType) -> PyResult<RequestBuilder> {
        let client = self.clone();

        let request = client.inner.client.request(method.0, url.0);
        let mut builder = RequestBuilder::new(client, request, self.inner.error_for_status);
        self.inner
            .total_timeout
            .map(|timeout| builder.inner_timeout(timeout))
            .transpose()?;
        self.inner
            .default_headers
            .as_ref()
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
        self.inner.runtime.close();
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
        let inner = Arc::new(ClientInner {
            client,
            runtime,
            middlewares,
            total_timeout,
            connection_limiter,
            error_for_status,
            default_headers,
            event_loop_cell: EventLoopCell::new(),
        });
        Client { inner }
    }

    pub fn spawn<F, T>(&self, future: F) -> PyResult<tokio::task::JoinHandle<T>>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.inner.runtime.spawn(future)
    }

    pub async fn execute_reqwest(&self, request: reqwest::Request) -> PyResult<reqwest::Response> {
        let client = &self.inner.client;
        client.execute(request).await.map_err(map_send_error)
    }

    pub async fn limit_connections(&self, request: &mut reqwest::Request) -> PyResult<Option<OwnedSemaphorePermit>> {
        if let Some(connection_limiter) = self.inner.connection_limiter.clone() {
            let req_timeout = request.timeout().copied();
            let (permit, elapsed) = connection_limiter.limit_connections(req_timeout).await?;

            if let Some(req_timeout) = req_timeout {
                if elapsed >= req_timeout {
                    return Err(PoolTimeoutError::from_causes("Timeout acquiring semaphore", Vec::new()));
                } else {
                    *request.timeout_mut() = Some(req_timeout - elapsed);
                }
            }
            Ok(Some(permit))
        } else {
            Ok(None)
        }
    }

    pub fn init_middleware_chain(&self) -> Option<Next> {
        if self.inner.middlewares.is_some() {
            Some(Next::new(self.clone()))
        } else {
            None
        }
    }

    pub fn get_middleware(&self, idx: usize) -> Option<&Py<PyAny>> {
        self.inner.middlewares.as_ref().map(|v| v.get(idx)).flatten()
    }

    pub fn get_event_loop(&self) -> &EventLoopCell {
        &self.inner.event_loop_cell
    }

    pub fn clone(&self) -> Self {
        let inner = Arc::clone(&self.inner);
        Client { inner }
    }
}
