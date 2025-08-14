use crate::client::Client;
use crate::http::{Body, Extensions, HeaderMap, Method, Url, UrlType};
use crate::middleware::Next;
use crate::response::{BodyConsumeConfig, Response};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::PyDict;

#[pyclass(subclass)]
pub struct Request {
    client: Client,
    inner: Option<reqwest::Request>,
    body: Option<Body>,
    py_body: Option<Py<Body>>,
    py_headers: Option<Py<HeaderMap>>,
    extensions: Option<Extensions>,
    error_for_status: bool,
    body_consume_config: BodyConsumeConfig,
}

#[pymethods]
impl Request {
    #[getter]
    fn get_method(&self) -> PyResult<Method> {
        Ok(self.inner_ref()?.method().clone().into())
    }

    #[setter]
    fn set_method(&mut self, value: Method) -> PyResult<()> {
        *self.inner_mut()?.method_mut() = value.0;
        Ok(())
    }

    #[getter]
    fn get_url(&self) -> PyResult<Url> {
        Ok(self.inner_ref()?.url().clone().into())
    }

    #[setter]
    fn set_url(&mut self, value: UrlType) -> PyResult<()> {
        *self.inner_mut()?.url_mut() = value.0;
        Ok(())
    }

    #[getter]
    fn get_headers(&mut self, py: Python) -> PyResult<&Py<HeaderMap>> {
        if self.py_headers.is_none() {
            let headers = HeaderMap::from(self.inner_ref()?.headers().clone());
            self.py_headers = Some(Py::new(py, headers)?);
        }
        Ok(&self.py_headers.as_ref().unwrap())
    }

    #[setter]
    fn set_headers(&mut self, py: Python, value: HeaderMap) -> PyResult<()> {
        self.py_headers = Some(value.into_pyobject(py)?.unbind());
        Ok(())
    }

    #[getter]
    fn get_body(&mut self) -> PyResult<Option<&Py<Body>>> {
        if let Some(body) = self.body.take() {
            self.py_body = Some(Python::with_gil(|py| Py::new(py, body))?);
        };
        Ok(self.py_body.as_ref())
    }

    #[setter]
    fn set_body(&mut self, value: Option<Bound<Body>>) -> PyResult<()> {
        self.body.take().map(drop);
        self.py_body.take().map(drop);
        self.py_body = value.map(|value| value.unbind());
        Ok(())
    }

    #[getter]
    fn get_extensions(&mut self) -> &Py<PyDict> {
        if self.extensions.is_none() {
            self.extensions = Some(Extensions(Python::with_gil(|py| PyDict::new(py).unbind())));
        }
        &self.extensions.as_ref().unwrap().0
    }

    #[setter]
    fn set_extensions(&mut self, value: Extensions) -> PyResult<()> {
        self.extensions = Some(value);
        Ok(())
    }

    fn copy(slf: Bound<Self>) -> PyResult<Bound<PyAny>> {
        slf.call_method0(intern!(slf.py(), "__copy__"))
    }

    fn __copy__(&mut self, _py: Python) -> PyResult<Self> {
        Err(PyNotImplementedError::new_err("Should be implemented in a subclass"))
    }
}
impl Request {
    pub fn new(
        client: Client,
        request: reqwest::Request,
        body: Option<Body>,
        extensions: Option<Extensions>,
        error_for_status: bool,
        body_consume_config: BodyConsumeConfig,
    ) -> Self {
        Request {
            client,
            inner: Some(request),
            extensions,
            body,
            py_body: None,
            py_headers: None,
            error_for_status,
            body_consume_config,
        }
    }

    pub async fn send_inner(slf: Py<PyAny>, with_middlewares: bool, cancel: CancelHandle) -> PyResult<Py<Response>> {
        let mut error_for_status = false;
        let mut middlewares_next = None;

        Python::with_gil(|py| -> PyResult<()> {
            let req = slf.bind(py).downcast::<Self>()?;
            let mut this = req.try_borrow_mut()?;
            let client = this.client.clone();

            error_for_status = this.error_for_status;
            if with_middlewares {
                middlewares_next = client.init_middleware_next(py)?;
            }

            if let Some(body) = this.body.as_mut() {
                body.set_task_local(py, Some(&client))?;
            }
            if let Some(body) = this.py_body.as_mut() {
                body.borrow_mut(py).set_task_local(py, Some(&client))?;
            }
            Ok(())
        })?;

        if let Some(middlewares_next) = middlewares_next {
            Next::call_first(middlewares_next, slf, error_for_status).await
        } else {
            let resp = Request::spawn_request(slf, cancel).await?;
            error_for_status.then(|| resp.error_for_status()).transpose()?;
            Python::with_gil(|py| Py::new(py, resp))
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

    async fn spawn_request(request: Py<PyAny>, cancel: CancelHandle) -> PyResult<Response> {
        let mut client = None;
        let mut inner_request = None;
        let mut extensions = None;
        let mut body_consume_config = BodyConsumeConfig::Fully;

        Python::with_gil(|py| -> PyResult<_> {
            let mut this = request.downcast_bound::<Request>(py)?.try_borrow_mut()?;
            client = Some(this.client.clone());
            extensions = this.extensions.take();
            body_consume_config = this.body_consume_config;

            let mut request = this
                .inner
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?;

            if let Some(py_headers) = this.py_headers.as_ref() {
                *request.headers_mut() = py_headers.try_borrow_mut(py)?.try_clone_inner()?;
            }

            if let Some(mut body) = this.body.take() {
                *request.body_mut() = Some(body.to_reqwest()?);
            } else if let Some(body) = this.py_body.take() {
                *request.body_mut() = Some(body.try_borrow_mut(py)?.to_reqwest()?);
            }
            inner_request = Some(request);
            Ok(())
        })?;

        client
            .unwrap()
            .spawn_reqwest(inner_request.unwrap(), body_consume_config, extensions, cancel)
            .await
    }

    pub fn try_clone_inner(&mut self, py: Python) -> PyResult<Self> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?;
        let new_inner = inner
            .try_clone()
            .ok_or_else(|| PyRuntimeError::new_err("Failed to clone request"))?;
        self.inner = Some(inner);

        let py_body = self
            .py_body
            .as_ref()
            .map(|b| Py::new(py, b.try_borrow(py)?.try_clone(py)?))
            .transpose()?;

        let py_headers = self
            .py_headers
            .as_ref()
            .map(|h| Py::new(py, h.try_borrow(py)?.try_clone()?))
            .transpose()?;

        Ok(Request {
            client: self.client.clone(),
            inner: Some(new_inner),
            body: self.body.as_ref().map(|b| b.try_clone(py)).transpose()?,
            py_body,
            py_headers,
            extensions: self.extensions.as_ref().map(|ext| ext.copy(py)).transpose()?,
            body_consume_config: self.body_consume_config,
            error_for_status: self.error_for_status,
        })
    }

    pub fn body_consume_config(&self) -> &BodyConsumeConfig {
        &self.body_consume_config
    }

    pub fn body_consume_config_mut(&mut self) -> &mut BodyConsumeConfig {
        &mut self.body_consume_config
    }
}
