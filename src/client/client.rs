use crate::asyncio::get_running_loop;
use crate::client::runtime::Handle;
use crate::exceptions::utils::map_send_error;
use crate::exceptions::{CloseError, PoolTimeoutError};
use crate::http::{Extensions, UrlType};
use crate::http::{HeaderMap, Method};
use crate::middleware::Next;
use crate::request::{ConnectionLimiter, RequestBuilder};
use crate::response::{BodyConsumeConfig, Response};
use pyo3::coroutine::CancelHandle;
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OwnedSemaphorePermit;
use tokio_util::sync::CancellationToken;

#[pyclass]
pub struct Client(Arc<ClientInner>);
struct ClientInner {
    client: reqwest::Client,
    runtime: Handle,
    middlewares: Option<Vec<Py<PyAny>>>,
    total_timeout: Option<Duration>,
    connection_limiter: Option<ConnectionLimiter>,
    error_for_status: bool,
    default_headers: Option<HeaderMap>,
    event_loop_cell: GILOnceCell<Py<PyAny>>,
    close_cancellation: CancellationToken,
}
impl Drop for ClientInner {
    fn drop(&mut self) {
        self.close_cancellation.cancel();
    }
}

#[pymethods]
impl Client {
    fn request(&self, method: Method, url: UrlType) -> PyResult<RequestBuilder> {
        let client = self.clone();

        let request = client.0.client.request(method.0, url.0);
        let mut builder = RequestBuilder::new(client, request, self.0.error_for_status);
        self.0
            .total_timeout
            .map(|timeout| builder.inner_timeout(timeout))
            .transpose()?;
        self.0
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
        self.0.close_cancellation.cancel();
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
    ) -> Self {
        Client(Arc::new(ClientInner {
            client,
            runtime,
            middlewares,
            total_timeout,
            connection_limiter,
            error_for_status,
            default_headers,
            event_loop_cell: GILOnceCell::new(),
            close_cancellation: CancellationToken::new(),
        }))
    }

    pub async fn spawn_reqwest(
        &self,
        mut request: reqwest::Request,
        body_consume_config: BodyConsumeConfig,
        extensions: Option<Extensions>,
        cancel: CancelHandle,
    ) -> PyResult<Response> {
        let client = self.0.client.clone();
        let connection_limiter = self.0.connection_limiter.clone();
        let close_handle = self.0.close_cancellation.clone();

        let fut = async move {
            let permit = match connection_limiter {
                Some(lim) => Some(Self::limit_connections(&lim, &mut request).await?),
                _ => None,
            };

            let mut resp = client.execute(request).await.map_err(map_send_error)?;

            extensions
                .map(|ext| ext.into_response(resp.extensions_mut()))
                .transpose()?;

            Response::initialize(resp, permit, body_consume_config).await
        };

        let fut = self.0.runtime.spawn(fut, cancel);

        tokio::select! {
            res = fut => res?,
            _ = close_handle.cancelled() => Err(CloseError::from_causes("Client was closed", vec![]),)
        }
    }

    pub async fn limit_connections(
        connection_limiter: &ConnectionLimiter,
        request: &mut reqwest::Request,
    ) -> PyResult<OwnedSemaphorePermit> {
        let req_timeout = request.timeout().copied();
        let now = std::time::Instant::now();

        let permit = connection_limiter.limit_connections(req_timeout).await?;
        let elapsed = now.elapsed();
        if let Some(req_timeout) = req_timeout {
            if elapsed >= req_timeout {
                return Err(PoolTimeoutError::from_causes("Timeout acquiring semaphore", vec![]));
            } else {
                *request.timeout_mut() = Some(req_timeout - elapsed);
            }
        }

        Ok(permit)
    }

    pub fn init_middleware_next(&self, py: Python) -> PyResult<Option<Py<Next>>> {
        if self.0.middlewares.is_some() {
            let task_local = Self::get_task_local_state(Some(self), py)?;
            let next = Py::new(py, Next::new(self.clone(), task_local))?;
            Ok(Some(next))
        } else {
            Ok(None)
        }
    }

    pub fn get_middleware(&self, idx: usize) -> Option<&Py<PyAny>> {
        self.0.middlewares.as_ref().map(|v| v.get(idx)).flatten()
    }

    pub fn get_task_local_state(client: Option<&Self>, py: Python) -> PyResult<TaskLocal> {
        static ONCE_CTX_VARS: GILOnceCell<Py<PyAny>> = GILOnceCell::new();

        let event_loop = match client {
            Some(client) => client
                .0
                .event_loop_cell
                .get_or_try_init(py, || Ok::<_, PyErr>(get_running_loop(py)?.unbind()))?
                .clone_ref(py),
            None => get_running_loop(py)?.unbind(),
        };

        Ok(TaskLocal {
            event_loop,
            context: ONCE_CTX_VARS
                .import(py, "contextvars", "copy_context")?
                .call0()?
                .unbind(),
        })
    }

    pub fn clone(&self) -> Self {
        Client(Arc::clone(&self.0))
    }
}

pub struct TaskLocal {
    pub event_loop: Py<PyAny>,
    pub context: Py<PyAny>,
}
impl TaskLocal {
    pub fn clone_ref(&self, py: Python) -> Self {
        TaskLocal {
            event_loop: self.event_loop.clone_ref(py),
            context: self.context.clone_ref(py),
        }
    }
}
