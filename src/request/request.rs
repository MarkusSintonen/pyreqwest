use crate::client::runtime::Runtime;
use crate::exceptions::SendError;
use crate::exceptions::utils::map_send_error;
use crate::http::body::Body;
use crate::http::types::{Extensions, HeaderMap, Method};
use crate::http::url::{Url, UrlType};
use crate::middleware::Next;
use crate::request::connection_limiter::ConnectionLimiter;
use crate::response::Response;
use futures_util::FutureExt;
use http::{HeaderName, HeaderValue};
use pyo3::PyResult;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::asyncio::CancelledError;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use std::str::FromStr;
use std::sync::Arc;

#[pyclass]
pub struct Request {
    runtime: Arc<Runtime>,
    client: Option<reqwest::Client>,
    inner: Option<reqwest::Request>,
    body: Option<Body>,
    extensions: Option<Extensions>,
    middlewares: Option<Arc<Vec<Py<PyAny>>>>,
    connection_limiter: Option<ConnectionLimiter>,
}

#[pymethods]
impl Request {
    pub async fn send(slf: Py<Self>, #[pyo3(cancel_handle)] mut cancel: CancelHandle) -> PyResult<Py<Response>> {
        let (runtime, middlewares) = Python::with_gil(|py| {
            let slf = slf.try_borrow(py)?;
            let runtime = slf.runtime.clone();
            let middlewares = slf.middlewares.clone();
            Ok::<_, PyErr>((runtime, middlewares))
        })?;

        let fut = async move {
            if let Some(middlewares) = middlewares {
                Next::execute_all(middlewares, slf).await
            } else {
                Request::execute(slf).await
            }
        };

        let join_handle = runtime.spawn(fut)?;

        tokio::select! {
            res = join_handle => res.map_err(|join_err| SendError::new_err(format!("Client was closed: {}", join_err)))?,
            _ = cancel.cancelled().fuse() => Err(CancelledError::new_err("Request was cancelled")),
        }
    }

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
                    .map(|s| s.to_string())
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
                    .map(|s| s.to_string())
                    .map_err(|e| PyRuntimeError::new_err(format!("Invalid header value: {}", e)))
            })
            .transpose()
    }

    pub fn copy_body(&self) -> PyResult<Option<Body>> {
        self.body.as_ref().map(|b| b.try_clone()).transpose()
    }

    pub fn set_body(&mut self, value: Bound<PyAny>) -> PyResult<()> {
        if value.is_none() {
            self.body = None;
        } else {
            self.body = Some(value.downcast::<Body>()?.try_borrow()?.try_clone()?);
        }
        Ok(())
    }

    pub fn copy_extensions(&self) -> Option<Extensions> {
        self.extensions.clone()
    }

    pub fn set_extensions(&mut self, value: Option<Extensions>) {
        self.extensions = value;
    }

    fn copy(&mut self) -> PyResult<Self> {
        self.try_clone()
    }

    fn __copy__(&mut self) -> PyResult<Self> {
        self.copy()
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
    ) -> Self {
        Request {
            runtime,
            client: Some(client),
            inner: Some(request),
            extensions,
            body,
            middlewares,
            connection_limiter,
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

    pub async fn execute(request: Py<Request>) -> PyResult<Py<Response>> {
        let (client, mut request, body, ext, conn_limiter) = Python::with_gil(|py| {
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
            Ok::<_, PyErr>((client, request, body, extensions, connection_limiter))
        })?;

        let permit = if let Some(connection_limiter) = conn_limiter.clone() {
            Some(connection_limiter.limit_connections().await?)
        } else {
            None
        };

        *request.body_mut() = body.map(|b| b.try_into()).transpose()?;
        let mut resp = client.execute(request).await.map_err(map_send_error)?;
        ext.map(|ext| Self::move_extensions(ext, resp.extensions_mut()));
        let resp = Response::initialize(resp, permit).await?;

        Python::with_gil(|py| Py::new(py, resp))
    }

    pub fn try_clone(&mut self) -> PyResult<Self> {
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
            body: self.body.as_ref().map(|b| b.try_clone()).transpose()?,
            extensions: self.extensions.clone(),
            connection_limiter: self.connection_limiter.clone(),
            middlewares: self.middlewares.clone(),
        })
    }

    fn move_extensions(from: Extensions, to: &mut http::Extensions) -> &mut Extensions {
        let to = to.get_or_insert_default::<Extensions>();
        for (k, v) in from.0.into_iter() {
            if !to.0.contains_key(&k) {
                to.0.insert(k, v);
            }
        }
        to
    }
}
