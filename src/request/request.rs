use crate::asyncio::EventLoopCell;
use crate::client::runtime::Runtime;
use crate::exceptions::utils::map_send_error;
use crate::exceptions::{CloseError, PoolTimeoutError, RequestPanicError};
use crate::http::Body;
use crate::http::{Extensions, HeaderMap, Method};
use crate::http::{Url, UrlType};
use crate::middleware::Next;
use crate::request::connection_limiter::ConnectionLimiter;
use crate::response::{ConsumeBodyConfig, Response};
use futures_util::FutureExt;
use http::{HeaderName, HeaderValue};
use pyo3::PyResult;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::asyncio::CancelledError;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::str::FromStr;
use std::sync::Arc;

#[pyclass(subclass)]
pub struct Request {
    runtime: Arc<Runtime>,
    client: Option<reqwest::Client>,
    inner: Option<reqwest::Request>,
    body: Option<Body>,
    extensions: Option<Extensions>,
    middlewares: Option<Arc<Vec<Py<PyAny>>>>,
    connection_limiter: Option<ConnectionLimiter>,
    error_for_status: bool,
    consume_body_config: ConsumeBodyConfig,
}

#[pymethods]
impl Request {
    #[getter]
    pub fn get_method(&self) -> PyResult<Method> {
        Ok(self.inner_ref()?.method().clone().into())
    }

    #[setter]
    pub fn set_method(&mut self, value: Method) -> PyResult<()> {
        *self.inner_mut()?.method_mut() = value.0;
        Ok(())
    }

    #[getter]
    pub fn get_url(&self) -> PyResult<Url> {
        Ok(self.inner_ref()?.url().clone().into())
    }

    #[setter]
    pub fn set_url(&mut self, value: UrlType) -> PyResult<()> {
        *self.inner_mut()?.url_mut() = value.0;
        Ok(())
    }

    #[getter]
    pub fn get_extensions(&mut self) -> &Py<PyDict> {
        if self.extensions.is_none() {
            self.extensions = Some(Extensions(Python::with_gil(|py| PyDict::new(py).unbind())));
        }
        &self.extensions.as_ref().unwrap().0
    }

    #[setter]
    pub fn set_extensions(&mut self, value: Extensions) -> PyResult<()> {
        self.extensions = Some(value);
        Ok(())
    }

    pub fn copy_headers(&self) -> PyResult<HeaderMap> {
        Ok(self.inner_ref()?.headers().clone().into())
    }

    pub fn set_headers(&mut self, value: HeaderMap) -> PyResult<()> {
        *self.inner_mut()?.headers_mut() = value.0;
        Ok(())
    }

    pub fn get_header(&self, key: &str) -> PyResult<Option<String>> {
        self.inner_ref()?
            .headers()
            .get(key)
            .map(|v| {
                v.to_str()
                    .map(ToString::to_string)
                    .map_err(|e| PyRuntimeError::new_err(format!("Invalid header value: {}", e)))
            })
            .transpose()
    }

    pub fn set_header(&mut self, key: &str, value: &str) -> PyResult<Option<String>> {
        let key =
            HeaderName::from_str(key).map_err(|e| PyValueError::new_err(format!("Invalid header name: {}", e)))?;
        let value =
            HeaderValue::from_str(value).map_err(|e| PyValueError::new_err(format!("Invalid header value: {}", e)))?;
        self.inner_mut()?
            .headers_mut()
            .insert(key, value)
            .map(|v| {
                v.to_str()
                    .map(ToString::to_string)
                    .map_err(|e| PyRuntimeError::new_err(format!("Invalid header value: {}", e)))
            })
            .transpose()
    }

    pub fn copy_body(&self, py: Python) -> PyResult<Option<Body>> {
        self.body.as_ref().map(|b| b.try_clone(py)).transpose()
    }

    pub fn set_body(&mut self, py: Python, value: Option<Bound<Body>>) -> PyResult<()> {
        self.body = value
            .map(|b| Ok::<_, PyErr>(b.try_borrow()?.try_clone(py)?))
            .transpose()?;
        Ok(())
    }

    fn copy(&mut self, py: Python) -> PyResult<Self> {
        self.try_clone(py)
    }

    fn __copy__(&mut self, py: Python) -> PyResult<Self> {
        self.try_clone(py)
    }
}
impl Request {
    pub fn new(
        runtime: Arc<Runtime>,
        client: reqwest::Client,
        request: reqwest::Request,
        body: Option<Body>,
        extensions: Option<Extensions>,
        middlewares: Option<Arc<Vec<Py<PyAny>>>>,
        connection_limiter: Option<ConnectionLimiter>,
        error_for_status: bool,
        consume_body_config: ConsumeBodyConfig,
    ) -> Self {
        Request {
            runtime,
            client: Some(client),
            inner: Some(request),
            extensions,
            body,
            middlewares,
            connection_limiter,
            error_for_status,
            consume_body_config,
        }
    }

    pub async fn send_inner(slf: &Py<PyAny>, mut cancel: CancelHandle) -> PyResult<Py<Response>> {
        let (slf, runtime, middlewares_next, error_for_status) = Python::with_gil(|py| {
            let req = slf.clone_ref(py).into_bound(py).downcast_into::<Self>()?;
            let mut req_borrow = req.try_borrow_mut()?;
            let runtime = req_borrow.runtime.clone();

            let mut ev_loop = EventLoopCell::new();
            let middlewares_next = req_borrow
                .middlewares
                .as_ref()
                .map(|middlewares| Next::py_new(py, middlewares.clone(), &mut ev_loop))
                .transpose()?;
            if let Some(body) = req_borrow.body.as_mut() {
                body.set_stream_event_loop(py, &mut ev_loop)?;
            }

            Ok::<_, PyErr>((req.unbind(), runtime, middlewares_next, req_borrow.error_for_status))
        })?;

        let fut = async move {
            let resp = if let Some(middlewares_next) = middlewares_next {
                let resp = Next::call_handle(middlewares_next, &slf).await?;
                error_for_status
                    .then(|| Python::with_gil(|py| resp.try_borrow_mut(py)?.error_for_status()))
                    .transpose()?;
                resp
            } else {
                let resp = Request::execute(&slf).await?;
                error_for_status.then(|| resp.error_for_status()).transpose()?;
                Python::with_gil(|py| Py::new(py, resp))?
            };
            Ok(resp)
        };

        let join_handle = runtime.spawn(fut)?;

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

    fn inner_ref(&self) -> PyResult<&reqwest::Request> {
        self.inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already consumed"))
    }

    fn inner_mut(&mut self) -> PyResult<&mut reqwest::Request> {
        self.inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already consumed"))
    }

    pub async fn execute(request: &Py<Request>) -> PyResult<Response> {
        let (client, mut request, body, ext, conn_limiter, consume_body) = Python::with_gil(|py| {
            let mut this = request.try_borrow_mut(py)?;
            let client = this
                .client
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?;
            let request = this
                .inner
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?;
            let body = this.body.take();
            let extensions = this.extensions.take();
            let connection_limiter = this.connection_limiter.take();
            let consume_body = this.consume_body_config;
            Ok::<_, PyErr>((client, request, body, extensions, connection_limiter, consume_body))
        })?;

        let permit = if let Some(connection_limiter) = conn_limiter.clone() {
            let req_timeout = request.timeout().copied();
            let (permit, elapsed) = connection_limiter.limit_connections(req_timeout).await?;

            if let Some(req_timeout) = req_timeout {
                if elapsed >= req_timeout {
                    return Err(PoolTimeoutError::from_causes("Timeout acquiring semaphore", Vec::new()));
                } else {
                    *request.timeout_mut() = Some(req_timeout - elapsed);
                }
            }
            Some(permit)
        } else {
            None
        };

        *request.body_mut() = body.map(|b| b.to_reqwest()).transpose()?;
        let mut resp = client.execute(request).await.map_err(map_send_error)?;
        ext.map(|ext| Self::move_extensions(ext, resp.extensions_mut()))
            .transpose()?;
        Response::initialize(resp, permit, consume_body).await
    }

    pub fn try_clone(&mut self, py: Python) -> PyResult<Self> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?;
        let new_inner = inner
            .try_clone()
            .ok_or_else(|| PyRuntimeError::new_err("Failed to clone request"))?;
        self.inner = Some(inner);

        Ok(Request {
            runtime: self.runtime.clone(),
            client: self.client.clone(),
            inner: Some(new_inner),
            body: self.body.as_ref().map(|b| b.try_clone(py)).transpose()?,
            extensions: self.extensions.as_ref().map(|ext| ext.copy_dict(py)).transpose()?,
            connection_limiter: self.connection_limiter.clone(),
            middlewares: self.middlewares.clone(),
            error_for_status: self.error_for_status,
            consume_body_config: self.consume_body_config,
        })
    }

    fn move_extensions(request_extensions: Extensions, response_extensions: &mut http::Extensions) -> PyResult<()> {
        Python::with_gil(|py| {
            let result_ext = request_extensions.0.into_bound(py);
            if let Some(resp_ext) = response_extensions.remove::<Extensions>() {
                let resp_ext = resp_ext.0.into_bound(py);
                result_ext.update(resp_ext.as_mapping())?;
            }
            response_extensions.insert(Extensions(result_ext.unbind()));
            Ok(())
        })
    }

    pub fn consume_body_config(&self) -> &ConsumeBodyConfig {
        &self.consume_body_config
    }

    pub fn consume_body_config_mut(&mut self) -> &mut ConsumeBodyConfig {
        &mut self.consume_body_config
    }
}
