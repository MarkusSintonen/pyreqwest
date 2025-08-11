use crate::asyncio::get_running_loop;
use crate::client::runtime::Runtime;
use crate::exceptions::utils::map_send_error;
use crate::exceptions::{CloseError, PoolTimeoutError, RequestPanicError};
use crate::http::UrlType;
use crate::http::{HeaderMap, Method};
use crate::middleware::Next;
use crate::request::{ConnectionLimiter, RequestBuilder};
use crate::response::ResponseBuilder;
use futures_util::FutureExt;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::asyncio::CancelledError;
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OwnedSemaphorePermit;

#[pyclass]
pub struct Client(Arc<ClientInner>);
struct ClientInner {
    client: reqwest::Client,
    runtime: Runtime,
    middlewares: Option<Vec<Py<PyAny>>>,
    total_timeout: Option<Duration>,
    connection_limiter: Option<ConnectionLimiter>,
    error_for_status: bool,
    default_headers: Option<HeaderMap>,
    event_loop_cell: GILOnceCell<Py<PyAny>>,
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
        self.0.runtime.close();
    }

    fn response_builder(&self) -> ResponseBuilder {
        ResponseBuilder::new(self.clone())
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
        Client(Arc::new(ClientInner {
            client,
            runtime,
            middlewares,
            total_timeout,
            connection_limiter,
            error_for_status,
            default_headers,
            event_loop_cell: GILOnceCell::new(),
        }))
    }

    pub async fn spawn<F, T>(&self, future: F, mut cancel: CancelHandle) -> PyResult<T>
    where
        F: Future<Output = PyResult<T>> + Send + 'static,
        T: Send + 'static,
    {
        let join_handle = self.0.runtime.spawn(future)?;

        tokio::select! {
            res = join_handle => res.map_err(|e| {
                match e.try_into_panic() {
                    Ok(payload) => RequestPanicError::from_panic_payload("Request panicked", payload),
                    Err(e) => CloseError::from_err("Client was closed", &e),
                }
            })?,
            _ = cancel.cancelled().fuse() => Err(CancelledError::new_err("Request was cancelled")),
        }
    }

    pub async fn execute_reqwest(&self, request: reqwest::Request) -> PyResult<reqwest::Response> {
        self.0.client.execute(request).await.map_err(map_send_error)
    }

    pub async fn limit_connections(&self, request: &mut reqwest::Request) -> PyResult<Option<OwnedSemaphorePermit>> {
        if let Some(connection_limiter) = self.0.connection_limiter.clone() {
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

    pub fn init_middleware_next(&self, py: Python) -> PyResult<Option<Py<Next>>> {
        if self.0.middlewares.is_some() {
            let task_local = self.get_task_local_state(py)?;
            let next = Py::new(py, Next::new(self.clone(), task_local))?;
            Ok(Some(next))
        } else {
            Ok(None)
        }
    }

    pub fn get_middleware(&self, idx: usize) -> Option<&Py<PyAny>> {
        self.0.middlewares.as_ref().map(|v| v.get(idx)).flatten()
    }

    pub fn get_task_local_state(&self, py: Python) -> PyResult<TaskLocal> {
        static ONCE_CTX_VARS: GILOnceCell<Py<PyAny>> = GILOnceCell::new();
        Ok(TaskLocal {
            event_loop: self
                .0
                .event_loop_cell
                .get_or_try_init(py, || Ok::<_, PyErr>(get_running_loop(py)?.unbind()))?
                .clone_ref(py),
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
