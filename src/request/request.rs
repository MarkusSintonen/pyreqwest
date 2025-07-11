use crate::Response;
use crate::client::runtime::Runtime;
use crate::exceptions::{PoolTimeoutError, SendError};
use crate::http_types::{Extensions, HeaderMapExt, MethodExt, UrlExt};
use crate::middleware::Next;
use crate::request::RequestBody;
use crate::request::RequestWrapper;
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
    async fn send(slf: Py<Self>, #[pyo3(cancel_handle)] mut cancel: CancelHandle) -> PyResult<Py<Response>> {
        let (fut, runtime) = Python::with_gil(|py| {
            let mut this = slf.try_borrow_mut(py)?;
            let client = this.client.clone();
            let runtime = this.runtime.clone();
            let middlewares = this.middlewares.clone();
            let request_semaphore = this.request_semaphore.clone();
            let connect_timeout = this.connect_timeout.clone();
            let mut request = this
                .inner
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?;

            let fut = async move {
                let permit = Self::limit_connections(request_semaphore, connect_timeout).await?;

                if let Some(middlewares) = middlewares {
                    Next::process(client, middlewares, request, permit).await
                } else {
                    let resp = request.execute(&client).await?;
                    let resp = Response::initialize(resp, permit).await?;
                    Python::with_gil(|py| Py::new(py, resp))
                }
            };
            Ok::<_, PyErr>((fut, runtime))
        })?;

        let join_handle = runtime.spawn(fut)?;

        tokio::select! {
            res = join_handle => res.map_err(|join_err| SendError::new_err(format!("Client was closed: {}", join_err)))?,
            _ = cancel.cancelled().fuse() => Err(CancelledError::new_err("Request was cancelled")),
        }
    }

    #[getter]
    fn get_method(&self) -> PyResult<MethodExt> {
        self.inner_ref()?.get_method()
    }

    #[setter]
    fn set_method(&mut self, value: MethodExt) -> PyResult<()> {
        self.inner_mut()?.set_method(value)
    }

    #[getter]
    fn get_url(&self) -> PyResult<UrlExt> {
        self.inner_ref()?.get_url()
    }

    #[setter]
    fn set_url(&mut self, value: UrlExt) -> PyResult<()> {
        self.inner_mut()?.set_url(value)
    }

    fn copy_headers(&self) -> PyResult<HeaderMapExt> {
        self.inner_ref()?.copy_headers()
    }

    fn set_headers(&mut self, value: HeaderMapExt) -> PyResult<()> {
        self.inner_mut()?.set_headers(value)
    }

    fn get_header(&self, key: &str) -> PyResult<Option<String>> {
        self.inner_ref()?.get_header(key)
    }

    fn set_header(&mut self, key: &str, value: &str) -> PyResult<Option<String>> {
        self.inner_mut()?.set_header(key, value)
    }

    fn copy_body(&self) -> PyResult<Option<RequestBody>> {
        self.inner_ref()?.copy_body()
    }

    fn set_body<'py>(&mut self, value: Bound<'py, PyAny>) -> PyResult<()> {
        self.inner_mut()?.set_body(value)
    }

    fn copy_extensions(&self) -> PyResult<Option<Extensions>> {
        Ok(self.inner_ref()?.copy_extensions())
    }

    fn set_extensions(&mut self, value: Option<Extensions>) -> PyResult<()> {
        Ok(self.inner_mut()?.set_extensions(value))
    }

    fn copy(&mut self) -> PyResult<Request> {
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

    fn __copy__(&mut self) -> PyResult<Request> {
        self.copy()
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

    fn inner_ref(&self) -> PyResult<&RequestWrapper> {
        self.inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already consumed"))
    }

    fn inner_mut(&mut self) -> PyResult<&mut RequestWrapper> {
        self.inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already consumed"))
    }
}
