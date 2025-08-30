use crate::asyncio::TaskLocal;
use crate::client::connection_limiter::ConnectionLimiter;
use crate::client::runtime::Handle;
use crate::client::spawner::Spawner;
use crate::http::{HeaderMap, Method};
use crate::http::{Url, UrlType};
use crate::middleware::Next;
use crate::request::RequestBuilder;
use pyo3::prelude::*;
use pyo3::{PyTraverseError, PyVisit};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[pyclass]
pub struct Client {
    client: reqwest::Client,
    base_url: Option<Url>,
    runtime: Handle,
    middlewares: Option<Arc<Vec<Py<PyAny>>>>,
    total_timeout: Option<Duration>,
    connection_limiter: Option<ConnectionLimiter>,
    error_for_status: bool,
    default_headers: Option<HeaderMap>,
    close_cancellation: CancellationToken,
}

#[pymethods]
impl Client {
    fn request(&self, method: Method, url: Bound<PyAny>) -> PyResult<RequestBuilder> {
        let middlewares_next = self.init_middleware_next(url.py())?;
        let spawner = Spawner::new(
            self.client.clone(),
            self.runtime.clone(),
            self.connection_limiter.clone(),
            self.close_cancellation.child_token(),
        );

        let url: reqwest::Url = match self.base_url.as_ref() {
            Some(base_url) => base_url.join(url.extract()?)?.into(),
            None => url.extract::<UrlType>()?.0,
        };

        let reqwest_request_builder = self.client.request(method.0, url);
        let mut builder =
            RequestBuilder::new(reqwest_request_builder, spawner, middlewares_next, self.error_for_status);

        self.total_timeout
            .map(|timeout| builder.inner_timeout(timeout))
            .transpose()?;
        self.default_headers
            .as_ref()
            .map(|default_headers| builder.inner_headers(default_headers))
            .transpose()?;
        Ok(builder)
    }

    pub fn get(&self, url: Bound<PyAny>) -> PyResult<RequestBuilder> {
        self.request(http::Method::GET.into(), url)
    }

    pub fn post(&self, url: Bound<PyAny>) -> PyResult<RequestBuilder> {
        self.request(http::Method::POST.into(), url)
    }

    pub fn put(&self, url: Bound<PyAny>) -> PyResult<RequestBuilder> {
        self.request(http::Method::PUT.into(), url)
    }

    pub fn patch(&self, url: Bound<PyAny>) -> PyResult<RequestBuilder> {
        self.request(http::Method::PATCH.into(), url)
    }

    pub fn delete(&self, url: Bound<PyAny>) -> PyResult<RequestBuilder> {
        self.request(http::Method::DELETE.into(), url)
    }

    pub fn head(&self, url: Bound<PyAny>) -> PyResult<RequestBuilder> {
        self.request(http::Method::HEAD.into(), url)
    }

    async fn __aenter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    async fn __aexit__(&self, _exc_type: Py<PyAny>, _exc_val: Py<PyAny>, _traceback: Py<PyAny>) {
        self.close().await;
    }

    async fn close(&self) {
        self.close_cancellation.cancel();
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Some(middlewares) = &self.middlewares {
            for mw in middlewares.iter() {
                visit.call(mw)?;
            }
        }
        Ok(())
    }

    fn __clear__(&mut self) {
        self.middlewares = None;
    }
}
impl Client {
    pub fn new(
        client: reqwest::Client,
        runtime: Handle,
        middlewares: Option<Vec<Py<PyAny>>>,
        total_timeout: Option<Duration>,
        connection_limiter: Option<ConnectionLimiter>,
        error_for_status: bool,
        default_headers: Option<HeaderMap>,
        base_url: Option<Url>,
    ) -> Self {
        Client {
            client,
            runtime,
            middlewares: middlewares.map(Arc::new),
            total_timeout,
            connection_limiter,
            error_for_status,
            default_headers,
            base_url,
            close_cancellation: CancellationToken::new(),
        }
    }

    pub fn init_middleware_next(&self, py: Python) -> PyResult<Option<Py<Next>>> {
        if let Some(middlewares) = self.middlewares.as_ref() {
            let task_local = TaskLocal::current(py)?;
            let next = Next::new(middlewares.clone(), task_local);
            Ok(Some(Py::new(py, next)?))
        } else {
            Ok(None)
        }
    }
}
