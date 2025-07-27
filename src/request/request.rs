use crate::asyncio::EventLoopCell;
use crate::client::runtime::Runtime;
use crate::exceptions::utils::map_send_error;
use crate::exceptions::{CloseError, PoolTimeoutError, RequestPanicError};
use crate::http::{Body, CIMultiDict};
use crate::http::{Extensions, HeaderMap, Method};
use crate::http::{Url, UrlType};
use crate::middleware::Next;
use crate::request::connection_limiter::ConnectionLimiter;
use crate::response::{ConsumeBodyConfig, Response};
use futures_util::FutureExt;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::exceptions::asyncio::CancelledError;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyMapping};
use std::sync::Arc;

#[pyclass(subclass)]
pub struct Request {
    runtime: Arc<Runtime>,
    client: Option<reqwest::Client>,
    inner: Option<reqwest::Request>,
    body: Option<Body>,
    py_body: Option<Py<Body>>,
    py_ci_multi_dict_headers: Option<Py<PyMapping>>,
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
    pub fn get_headers(&mut self) -> PyResult<&Py<PyMapping>> {
        if self.py_ci_multi_dict_headers.is_none() {
            let headers = Python::with_gil(|py| -> PyResult<_> {
                Ok(CIMultiDict::new(py, self.inner_ref()?.headers())?
                    .into_pyobject(py)?
                    .unbind())
            })?;
            self.py_ci_multi_dict_headers = Some(headers);
        }
        Ok(&self.py_ci_multi_dict_headers.as_ref().unwrap())
    }

    #[setter]
    pub fn set_headers(&mut self, py: Python, value: HeaderMap) -> PyResult<()> {
        self.py_ci_multi_dict_headers = Some(value.into_pyobject(py)?.unbind());
        Ok(())
    }

    #[getter]
    pub fn get_body(&mut self) -> PyResult<Option<&Py<Body>>> {
        if let Some(body) = self.body.take() {
            self.py_body = Some(Python::with_gil(|py| Py::new(py, body))?);
        };
        Ok(self.py_body.as_ref())
    }

    #[setter]
    pub fn set_body(&mut self, value: Option<Bound<Body>>) -> PyResult<()> {
        self.body.take().map(drop);
        self.py_body.take().map(drop);
        self.py_body = value.map(|value| value.unbind());
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
            py_body: None,
            py_ci_multi_dict_headers: None,
            middlewares,
            connection_limiter,
            error_for_status,
            consume_body_config,
        }
    }

    pub async fn send_inner(slf: &Py<PyAny>, mut cancel: CancelHandle) -> PyResult<Py<Response>> {
        struct SendParams {
            request: Py<Request>,
            runtime: Arc<Runtime>,
            middlewares_next: Option<Py<Next>>,
            error_for_status: bool,
        }

        let params = Python::with_gil(|py| {
            let req = slf.clone_ref(py).into_bound(py).downcast_into::<Self>()?;
            let mut req_borrow = req.try_borrow_mut()?;

            let mut ev_loop = EventLoopCell::new();
            let middlewares_next = req_borrow
                .middlewares
                .as_ref()
                .map(|middlewares| Next::py_new(py, middlewares.clone(), &mut ev_loop))
                .transpose()?;
            if let Some(body) = req_borrow.body.as_mut() {
                body.set_stream_event_loop(py, &mut ev_loop)?;
            }
            if let Some(body) = req_borrow.py_body.as_mut() {
                body.borrow_mut(py).set_stream_event_loop(py, &mut ev_loop)?;
            }

            let params = SendParams {
                request: req.unbind(),
                runtime: req_borrow.runtime.clone(),
                middlewares_next,
                error_for_status: req_borrow.error_for_status,
            };
            Ok::<_, PyErr>(params)
        })?;

        let fut = async move {
            let resp = if let Some(middlewares_next) = params.middlewares_next {
                let resp = Next::call_handle(middlewares_next, &params.request).await?;
                params
                    .error_for_status
                    .then(|| Python::with_gil(|py| resp.try_borrow_mut(py)?.error_for_status()))
                    .transpose()?;
                resp
            } else {
                let resp = Request::execute(&params.request).await?;
                params.error_for_status.then(|| resp.error_for_status()).transpose()?;
                Python::with_gil(|py| Py::new(py, resp))?
            };
            Ok(resp)
        };

        let join_handle = params.runtime.spawn(fut)?;

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
        struct ExecParams {
            client: reqwest::Client,
            request: reqwest::Request,
            extensions: Option<Extensions>,
            connection_limiter: Option<ConnectionLimiter>,
            consume_body_config: ConsumeBodyConfig,
        }

        let mut params = Python::with_gil(|py| {
            let mut this = request.try_borrow_mut(py)?;
            let mut request = this
                .inner
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?;

            if let Some(py_headers) = this.py_ci_multi_dict_headers.take() {
                let headers: CIMultiDict = py_headers.extract(py)?;
                *request.headers_mut() = headers.to_http_header_map(py)?;
            }

            if let Some(mut body) = this.body.take() {
                *request.body_mut() = Some(body.to_reqwest()?);
            } else if let Some(body) = this.py_body.take() {
                *request.body_mut() = Some(body.try_borrow_mut(py)?.to_reqwest()?);
            }

            let params = ExecParams {
                request,
                client: this
                    .client
                    .take()
                    .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?,
                extensions: this.extensions.take(),
                connection_limiter: this.connection_limiter.take(),
                consume_body_config: this.consume_body_config,
            };
            Ok::<_, PyErr>(params)
        })?;

        let permit = if let Some(connection_limiter) = params.connection_limiter.clone() {
            let req_timeout = params.request.timeout().copied();
            let (permit, elapsed) = connection_limiter.limit_connections(req_timeout).await?;

            if let Some(req_timeout) = req_timeout {
                if elapsed >= req_timeout {
                    return Err(PoolTimeoutError::from_causes("Timeout acquiring semaphore", Vec::new()));
                } else {
                    *params.request.timeout_mut() = Some(req_timeout - elapsed);
                }
            }
            Some(permit)
        } else {
            None
        };

        let mut resp = params.client.execute(params.request).await.map_err(map_send_error)?;

        params
            .extensions
            .map(|ext| Self::move_extensions(ext, resp.extensions_mut()))
            .transpose()?;

        Response::initialize(resp, permit, params.consume_body_config).await
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
            py_body: self
                .py_body
                .as_ref()
                .map(|b| Py::new(py, b.try_borrow(py)?.try_clone(py)?))
                .transpose()?,
            py_ci_multi_dict_headers: self
                .py_ci_multi_dict_headers
                .as_ref()
                .map(|h| -> PyResult<_> {
                    Ok(h.bind(py)
                        .call_method0(intern!(py, "__copy__"))?
                        .downcast_into::<PyMapping>()?
                        .unbind())
                })
                .transpose()?,
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
