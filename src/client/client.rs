use crate::client::connection_limiter::ConnectionLimiter;
use crate::client::runtime::Handle;
use crate::client::spawner::Spawner;
use crate::http::{HeaderMap, Method};
use crate::http::{Url, UrlType};
use crate::middleware::NextInner;
use crate::request::{BaseRequestBuilder, BlockingRequestBuilder, RequestBuilder};
use pyo3::prelude::*;
use pyo3::{PyTraverseError, PyVisit};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[pyclass(subclass, frozen)]
pub struct BaseClient {
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

#[pyclass(extends=BaseClient, frozen)]
pub struct Client;

#[pyclass(extends=BaseClient, frozen)]
pub struct BlockingClient;

#[pymethods]
impl BaseClient {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Some(middlewares) = &self.middlewares {
            for mw in middlewares.iter() {
                visit.call(mw)?;
            }
        }
        Ok(())
    }
}
impl BaseClient {
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
        BaseClient {
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

    pub fn create_request_builder(&self, method: Method, url: Bound<PyAny>) -> PyResult<BaseRequestBuilder> {
        let py = url.py();

        let url: reqwest::Url = match self.base_url.as_ref() {
            Some(base_url) => base_url.join(url.extract()?)?.into(),
            None => url.extract::<UrlType>()?.0,
        };

        py.detach(|| {
            let spawner = Spawner::new(
                self.client.clone(),
                self.runtime.clone(),
                self.connection_limiter.clone(),
                self.close_cancellation.child_token(),
            );

            let reqwest_request_builder = self.client.request(method.0, url);
            let middlewares_next = self.init_middleware_next()?;

            let mut builder =
                BaseRequestBuilder::new(reqwest_request_builder, spawner, middlewares_next, self.error_for_status);

            self.total_timeout
                .map(|timeout| builder.inner_timeout(timeout))
                .transpose()?;
            self.default_headers
                .as_ref()
                .map(|default_headers| builder.inner_headers(default_headers))
                .transpose()?;
            Ok(builder)
        })
    }

    pub fn init_middleware_next(&self) -> PyResult<Option<NextInner>> {
        self.middlewares
            .as_ref()
            .map(|middlewares| NextInner::new(middlewares.clone()))
            .transpose()
    }
}

#[pymethods]
impl Client {
    pub fn request(slf: PyRef<Self>, method: Method, url: Bound<PyAny>) -> PyResult<Py<RequestBuilder>> {
        let builder = slf.as_super().create_request_builder(method, url)?;
        RequestBuilder::new_py(slf.py(), builder)
    }

    pub fn get(slf: PyRef<Self>, url: Bound<PyAny>) -> PyResult<Py<RequestBuilder>> {
        Self::request(slf, http::Method::GET.into(), url)
    }

    pub fn post(slf: PyRef<Self>, url: Bound<PyAny>) -> PyResult<Py<RequestBuilder>> {
        Self::request(slf, http::Method::POST.into(), url)
    }

    pub fn put(slf: PyRef<Self>, url: Bound<PyAny>) -> PyResult<Py<RequestBuilder>> {
        Self::request(slf, http::Method::PUT.into(), url)
    }

    pub fn patch(slf: PyRef<Self>, url: Bound<PyAny>) -> PyResult<Py<RequestBuilder>> {
        Self::request(slf, http::Method::PATCH.into(), url)
    }

    pub fn delete(slf: PyRef<Self>, url: Bound<PyAny>) -> PyResult<Py<RequestBuilder>> {
        Self::request(slf, http::Method::DELETE.into(), url)
    }

    pub fn head(slf: PyRef<Self>, url: Bound<PyAny>) -> PyResult<Py<RequestBuilder>> {
        Self::request(slf, http::Method::HEAD.into(), url)
    }

    async fn __aenter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    async fn __aexit__(
        slf: Py<Self>,
        _exc_type: Py<PyAny>,
        _exc_val: Py<PyAny>,
        _traceback: Py<PyAny>,
    ) -> PyResult<()> {
        Self::close(slf).await
    }

    async fn close(slf: Py<Self>) -> PyResult<()> {
        // Currently, does not wait for resources to be released.
        Python::attach(|py| {
            slf.bind(py).as_super().try_borrow()?.close_cancellation.cancel();
            Ok(())
        })
    }
}
impl Client {
    pub fn new_py(py: Python, inner: BaseClient) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self))
    }
}

#[pymethods]
impl BlockingClient {
    pub fn request(slf: PyRef<Self>, method: Method, url: Bound<PyAny>) -> PyResult<Py<BlockingRequestBuilder>> {
        let builder = slf.as_super().create_request_builder(method, url)?;
        BlockingRequestBuilder::new_py(slf.py(), builder)
    }

    pub fn get(slf: PyRef<Self>, url: Bound<PyAny>) -> PyResult<Py<BlockingRequestBuilder>> {
        Self::request(slf, http::Method::GET.into(), url)
    }

    pub fn post(slf: PyRef<Self>, url: Bound<PyAny>) -> PyResult<Py<BlockingRequestBuilder>> {
        Self::request(slf, http::Method::POST.into(), url)
    }

    pub fn put(slf: PyRef<Self>, url: Bound<PyAny>) -> PyResult<Py<BlockingRequestBuilder>> {
        Self::request(slf, http::Method::PUT.into(), url)
    }

    pub fn patch(slf: PyRef<Self>, url: Bound<PyAny>) -> PyResult<Py<BlockingRequestBuilder>> {
        Self::request(slf, http::Method::PATCH.into(), url)
    }

    pub fn delete(slf: PyRef<Self>, url: Bound<PyAny>) -> PyResult<Py<BlockingRequestBuilder>> {
        Self::request(slf, http::Method::DELETE.into(), url)
    }

    pub fn head(slf: PyRef<Self>, url: Bound<PyAny>) -> PyResult<Py<BlockingRequestBuilder>> {
        Self::request(slf, http::Method::HEAD.into(), url)
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __exit__(slf: PyRef<Self>, _exc_type: Py<PyAny>, _exc_val: Py<PyAny>, _traceback: Py<PyAny>) {
        Self::close(slf)
    }

    fn close(slf: PyRef<Self>) {
        slf.as_super().close_cancellation.cancel();
    }
}
impl BlockingClient {
    pub fn new_py(py: Python, inner: BaseClient) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self))
    }
}
