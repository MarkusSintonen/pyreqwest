use crate::client::Client;
use crate::http::{Body, Extensions, HeaderMap, Method, Url, UrlType};
use crate::response::{BodyConsumeConfig, Response};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyType};

#[pyclass(subclass)]
pub struct Request {
    client: Client,
    inner: Option<reqwest::Request>,
    body: Option<ReqBody>,
    py_headers: Option<Py<HeaderMap>>,
    extensions: Option<Extensions>,
    error_for_status: bool,
    body_consume_config: BodyConsumeConfig,

    #[pyo3(get, set)]
    _interceptor: Option<Py<PyAny>>,
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
    fn get_body(&mut self, py: Python) -> PyResult<Option<Py<Body>>> {
        match self.body.as_mut() {
            Some(ReqBody::Body(body)) => {
                let py_body = Py::new(py, body.take_inner()?)?;
                self.body = Some(ReqBody::PyBody(py_body.clone_ref(py)));
                Ok(Some(py_body))
            }
            Some(ReqBody::PyBody(py_body)) => Ok(Some(py_body.clone_ref(py))),
            None => Ok(None),
        }
    }

    #[setter]
    fn set_body(&mut self, body: Option<Py<Body>>) -> PyResult<()> {
        self.body = body.map(ReqBody::PyBody);
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

    #[classmethod]
    pub fn from_request_and_body(
        _cls: &Bound<'_, PyType>,
        _py: Python,
        _request: Bound<PyAny>,
        _body: Option<Bound<Body>>,
    ) -> PyResult<Self> {
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
            body: body.map(ReqBody::Body),
            py_headers: None,
            error_for_status,
            body_consume_config,
            _interceptor: None,
        }
    }

    pub async fn send_inner(slf: Py<PyAny>, cancel: CancelHandle) -> PyResult<Py<Response>> {
        let mut error_for_status = false;
        let mut middlewares_next = None;

        Python::with_gil(|py| -> PyResult<()> {
            let req = slf.bind(py).downcast::<Self>()?;
            let mut this = req.try_borrow_mut()?;
            let client = this.client.clone();

            error_for_status = this.error_for_status;
            middlewares_next = client.init_middleware_next(py, this.override_middlewares(py, &client)?)?;

            match this.body.as_mut() {
                Some(ReqBody::Body(body)) => body.set_task_local(py, Some(&client))?,
                Some(ReqBody::PyBody(py_body)) => py_body.borrow_mut(py).set_task_local(py, Some(&client))?,
                None => {}
            }
            Ok(())
        })?;

        match middlewares_next {
            Some(middlewares_next) => middlewares_next.call_first(slf, error_for_status).await,
            None => Request::spawn_request(slf, cancel).await,
        }
    }

    fn override_middlewares(&self, py: Python, client: &Client) -> PyResult<Option<Vec<Py<PyAny>>>> {
        if let Some(interceptor) = self._interceptor.as_ref() {
            let mut override_middlewares = Vec::new();
            if let Some(middlewares) = client.middlewares() {
                override_middlewares.extend(middlewares.iter().map(|m| m.clone_ref(py)));
            }
            override_middlewares.push(interceptor.clone_ref(py));
            Ok(Some(override_middlewares))
        } else {
            Ok(None)
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

    pub async fn spawn_request(request: Py<PyAny>, cancel: CancelHandle) -> PyResult<Py<Response>> {
        let mut client = None;
        let mut inner_request = None;
        let mut extensions = None;
        let mut error_for_status = false;
        let mut body_consume_config = BodyConsumeConfig::Fully;

        Python::with_gil(|py| -> PyResult<_> {
            let mut this = request.downcast_bound::<Request>(py)?.try_borrow_mut()?;
            client = Some(this.client.clone());
            extensions = this.extensions.take();
            error_for_status = this.error_for_status;
            body_consume_config = this.body_consume_config;

            let mut request = this
                .inner
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?;

            if let Some(py_headers) = this.py_headers.as_ref() {
                *request.headers_mut() = py_headers.try_borrow_mut(py)?.try_take_inner()?;
            }

            match this.body.take() {
                Some(ReqBody::Body(mut body)) => {
                    body.set_task_local(py, client.as_ref())?;
                    *request.body_mut() = Some(body.to_reqwest()?)
                }
                Some(ReqBody::PyBody(py_body)) => {
                    let mut py_body = py_body.try_borrow_mut(py)?;
                    py_body.set_task_local(py, client.as_ref())?;
                    *request.body_mut() = Some(py_body.to_reqwest()?)
                }
                None => {}
            }
            inner_request = Some(request);
            Ok(())
        })?;

        let resp = client
            .unwrap()
            .spawn_reqwest(inner_request.unwrap(), body_consume_config, extensions, cancel)
            .await?;

        error_for_status.then(|| resp.error_for_status()).transpose()?;

        Python::with_gil(|py| Py::new(py, resp))
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

        let body = match self.body.as_ref() {
            Some(ReqBody::Body(body)) => Some(body.try_clone(py)?),
            Some(ReqBody::PyBody(py_body)) => Some(py_body.borrow_mut(py).try_clone(py)?),
            None => None,
        };

        let py_headers = self
            .py_headers
            .as_ref()
            .map(|h| Py::new(py, h.try_borrow(py)?.try_clone()?))
            .transpose()?;

        Ok(Request {
            client: self.client.clone(),
            inner: Some(new_inner),
            body: body.map(ReqBody::Body),
            py_headers,
            extensions: self.extensions.as_ref().map(|ext| ext.copy(py)).transpose()?,
            error_for_status: self.error_for_status,
            body_consume_config: self.body_consume_config,
            _interceptor: self._interceptor.as_ref().map(|v| v.clone_ref(py)),
        })
    }

    pub fn body_consume_config(&self) -> &BodyConsumeConfig {
        &self.body_consume_config
    }

    pub fn body_consume_config_mut(&mut self) -> &mut BodyConsumeConfig {
        &mut self.body_consume_config
    }

    pub fn inner_from_request_and_body(py: Python, request: Bound<PyAny>, body: Option<Bound<Body>>) -> PyResult<Self> {
        let this = request.downcast::<Request>()?.try_borrow()?;
        let client = this.client.clone();
        let inner_request = this
            .inner_ref()?
            .try_clone()
            .ok_or_else(|| PyRuntimeError::new_err("Failed to clone request"))?;

        let body = body.map(|b| b.try_borrow_mut()?.take_inner()).transpose()?;

        Ok(Request::new(
            client,
            inner_request,
            body,
            this.extensions.as_ref().map(|ext| ext.copy(py)).transpose()?,
            this.error_for_status,
            this.body_consume_config.clone(),
        ))
    }
}

enum ReqBody {
    Body(Body),
    PyBody(Py<Body>), // In Python heap
}
