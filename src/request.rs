use crate::Response;
use crate::exceptions::{PoolTimeoutError, SendError};
use crate::middleware::Next;
use crate::request_wrapper::RequestWrapper;
use crate::runtime::Runtime;
use futures_util::FutureExt;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::exceptions::asyncio::CancelledError;
use pyo3::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[pyclass]
pub struct Request {
    runtime: Arc<Runtime>,
    client: Arc<reqwest::Client>,
    inner: Option<RequestWrapper>,
    middlewares: Option<Arc<Vec<Py<PyAny>>>>,
    request_semaphore: Option<Arc<Semaphore>>,
    connect_timeout: Option<Duration>,
}
#[pymethods]
impl Request {
    async fn send(&mut self, #[pyo3(cancel_handle)] mut cancel: CancelHandle) -> PyResult<Response> {
        let client = self.client.clone();
        let middlewares = self.middlewares.clone();
        let request_semaphore = self.request_semaphore.clone();
        let connect_timeout = self.connect_timeout.clone();

        let mut request = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?;

        let join_handle = self.runtime.spawn(async move {
            let permit = Self::limit_connections(request_semaphore, connect_timeout).await?;

            let response = if let Some(middlewares) = middlewares {
                Next::process(client, middlewares, request).await?
            } else {
                request.execute(&client).await?
            };

            Response::initialize(response, permit).await
        })?;

        tokio::select! {
            res = join_handle => res.map_err(|join_err| SendError::new_err(format!("Client was closed: {}", join_err)))?,
            _ = cancel.cancelled().fuse() => Err(CancelledError::new_err("Request was cancelled")),
        }
    }

    fn __copy__(&mut self) -> PyResult<Request> {
        let mut inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?;
        let new_inner = inner.try_clone()?;
        self.inner = Some(inner);

        Ok(Request {
            runtime: self.runtime.clone(),
            client: self.client.clone(),
            inner: Some(new_inner),
            middlewares: self.middlewares.clone(),
            request_semaphore: self.request_semaphore.clone(),
            connect_timeout: self.connect_timeout.clone(),
        })
    }
}
impl Request {
    pub fn new(
        runtime: Arc<Runtime>,
        client: reqwest::Client,
        inner: RequestWrapper,
        middlewares: Option<Arc<Vec<Py<PyAny>>>>,
        request_semaphore: Option<Arc<Semaphore>>,
        connect_timeout: Option<Duration>,
    ) -> Self {
        Self {
            runtime,
            client: Arc::new(client),
            inner: Some(inner),
            middlewares,
            request_semaphore,
            connect_timeout,
        }
    }

    async fn limit_connections(
        semaphore: Option<Arc<Semaphore>>,
        timeout: Option<Duration>,
    ) -> PyResult<Option<OwnedSemaphorePermit>> {
        let Some(request_semaphore) = semaphore.clone() else {
            return Ok(None);
        };
        let permit = if let Some(timeout) = timeout {
            tokio::time::timeout(timeout, request_semaphore.acquire_owned())
                .await
                .map_err(|_| PoolTimeoutError::new_err("Timeout acquiring semaphore"))?
        } else {
            request_semaphore.acquire_owned().await
        };
        permit
            .map(Some)
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to acquire semaphore: {}", e)))
    }
}
