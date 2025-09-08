use crate::allow_threads::AllowThreads;
use crate::client::{SpawnRequestData, Spawner};
use crate::http::{Extensions, HeaderMap, Method, RequestBody, Url, UrlType};
use crate::middleware::{BlockingNext, Next, NextInner};
use crate::response::{BlockingResponse, BodyConsumeConfig, Response};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString, PyType};
use pyo3::{PyTraverseError, PyVisit, intern};
use std::fmt::Display;

#[pyclass(subclass)]
pub struct Request {
    inner: Option<reqwest::Request>,
    spawner: Spawner,
    body: Option<ReqBody>,
    headers: Option<ReqHeaders>,
    extensions: Option<Extensions>,
    middlewares_next: Option<NextInner>,
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
        if self.headers.is_none() {
            let headers = HeaderMap::from(self.inner_ref()?.headers().clone());
            self.headers = Some(ReqHeaders::PyHeaders(Py::new(py, headers)?));
        }
        if let Some(ReqHeaders::Headers(h)) = &self.headers {
            let py_headers = Py::new(py, HeaderMap::from(h.try_take_inner()?))?;
            self.headers = Some(ReqHeaders::PyHeaders(py_headers.clone_ref(py)));
        }
        match self.headers.as_ref() {
            Some(ReqHeaders::PyHeaders(h)) => Ok(h),
            _ => unreachable!(),
        }
    }

    #[setter]
    fn set_headers(&mut self, py: Python, value: HeaderMap) -> PyResult<()> {
        self.headers = Some(ReqHeaders::PyHeaders(Py::new(py, value)?));
        Ok(())
    }

    #[getter]
    fn get_body(&mut self, py: Python) -> PyResult<Option<Py<RequestBody>>> {
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
    fn set_body(&mut self, body: Option<Py<RequestBody>>) -> PyResult<()> {
        self.body = body.map(ReqBody::PyBody);
        Ok(())
    }

    #[getter]
    fn get_extensions(&mut self) -> &Py<PyDict> {
        if self.extensions.is_none() {
            self.extensions = Some(Extensions(Python::attach(|py| PyDict::new(py).unbind())));
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

    pub fn __repr__(&self, py: Python) -> PyResult<String> {
        self.repr(py, true)
    }

    fn repr_full(&self, py: Python) -> PyResult<String> {
        self.repr(py, false)
    }

    #[getter]
    fn get_read_buffer_limit(&self) -> PyResult<usize> {
        match self.body_consume_config() {
            BodyConsumeConfig::Streamed(conf) => Ok(conf.read_buffer_limit),
            BodyConsumeConfig::FullyConsumed => Err(PyRuntimeError::new_err("Unexpected config")),
        }
    }

    #[classmethod]
    pub fn from_request_and_body(
        _cls: &Bound<'_, PyType>,
        _py: Python,
        _request: Bound<PyAny>,
        _body: Option<Bound<RequestBody>>,
    ) -> PyResult<Self> {
        Err(PyNotImplementedError::new_err("Should be implemented in a subclass"))
    }

    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Some(ReqHeaders::PyHeaders(py_headers)) = &self.headers {
            visit.call(py_headers)?;
        }
        if let Some(extensions) = &self.extensions {
            visit.call(&extensions.0)?;
        }
        if let Some(middlewares_next) = &self.middlewares_next {
            middlewares_next.__traverse__(&visit)?;
        }
        match &self.body {
            Some(ReqBody::Body(body)) => body.__traverse__(visit)?,
            Some(ReqBody::PyBody(py_body)) => visit.call(py_body)?,
            None => {}
        }
        Ok(())
    }

    fn __clear__(&mut self) {
        self.inner = None;
        self.body = None;
        self.headers = None;
        self.extensions = None;
        self.middlewares_next = None;
    }
}
impl Request {
    pub fn new(
        request: reqwest::Request,
        spawner: Spawner,
        body: Option<RequestBody>,
        extensions: Option<Extensions>,
        middlewares_next: Option<NextInner>,
        error_for_status: bool,
        body_consume_config: BodyConsumeConfig,
    ) -> Self {
        Request {
            inner: Some(request),
            spawner,
            extensions,
            body: body.map(ReqBody::Body),
            headers: None,
            middlewares_next,
            error_for_status,
            body_consume_config,
        }
    }

    pub async fn send_inner(py_request: &Py<PyAny>, cancel: CancelHandle) -> PyResult<Py<Response>> {
        let (middlewares_next, error_for_status) = Python::attach(|py| {
            let mut this = py_request.bind(py).downcast::<Self>()?.try_borrow_mut()?;
            this.body_set_task_local()?;

            Ok::<_, PyErr>((this.middlewares_next.take().map(|m| Next::new(m, py)).transpose()?, this.error_for_status))
        })?;

        match middlewares_next {
            Some(middlewares_next) => {
                let middleware_resp = AllowThreads(middlewares_next.run_inner(py_request, cancel)).await?;

                if error_for_status {
                    Python::attach(|py| middleware_resp.bind(py).as_super().try_borrow()?.error_for_status())?;
                }
                Ok(middleware_resp)
            }
            None => Self::spawn_request(py_request, cancel).await,
        }
    }

    pub fn blocking_send_inner(py_request: &Py<PyAny>) -> PyResult<Py<BlockingResponse>> {
        let (middlewares_next, error_for_status) = Python::attach(|py| {
            let mut this = py_request.bind(py).downcast::<Self>()?.try_borrow_mut()?;
            this.body_set_task_local()?;

            Ok::<_, PyErr>((this.middlewares_next.take().map(BlockingNext::new).transpose()?, this.error_for_status))
        })?;

        match middlewares_next {
            Some(middlewares_next) => Python::attach(|py| {
                let middleware_resp = middlewares_next.run(py_request.bind(py))?;
                if error_for_status {
                    middleware_resp.bind(py).as_super().try_borrow()?.error_for_status()?;
                }
                Ok(middleware_resp)
            }),
            None => Self::blocking_spawn_request(py_request),
        }
    }

    fn body_set_task_local(&self) -> PyResult<()> {
        match self.body.as_ref() {
            Some(ReqBody::Body(body)) => body.set_task_local(),
            Some(ReqBody::PyBody(py_body)) => py_body.get().set_task_local(),
            None => Ok(()),
        }
    }

    pub async fn spawn_request(request: &Py<PyAny>, cancel: CancelHandle) -> PyResult<Py<Response>> {
        Spawner::spawn_reqwest(Self::prepare_spawn_request(request)?, cancel).await
    }

    pub fn blocking_spawn_request(request: &Py<PyAny>) -> PyResult<Py<BlockingResponse>> {
        Spawner::blocking_spawn_reqwest(Self::prepare_spawn_request(request)?)
    }

    fn prepare_spawn_request(request: &Py<PyAny>) -> PyResult<SpawnRequestData> {
        Python::attach(|py| -> PyResult<_> {
            let mut this = request.downcast_bound::<Self>(py)?.try_borrow_mut()?;
            let mut request = this
                .inner
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?;

            match this.headers.as_ref() {
                Some(ReqHeaders::Headers(h)) => {
                    *request.headers_mut() = h.try_take_inner()?;
                    this.headers = None;
                }
                Some(ReqHeaders::PyHeaders(py_headers)) => {
                    *request.headers_mut() = py_headers.get().try_take_inner()?;
                }
                None => {}
            }

            match this.body.take() {
                Some(ReqBody::Body(body)) => {
                    body.set_task_local()?;
                    *request.body_mut() = Some(body.into_reqwest()?)
                }
                Some(ReqBody::PyBody(py_body)) => {
                    let py_body = py_body.get();
                    py_body.set_task_local()?;
                    *request.body_mut() = Some(py_body.into_reqwest()?)
                }
                None => {}
            }

            Ok(SpawnRequestData {
                request,
                spawner: this.spawner.clone(),
                extensions: this.extensions.take(),
                error_for_status: this.error_for_status,
                body_consume_config: this.body_consume_config,
            })
        })
    }

    pub fn try_clone_inner(&self, py: Python) -> PyResult<Self> {
        let new_inner = self
            .inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?
            .try_clone()
            .ok_or_else(|| PyRuntimeError::new_err("Failed to clone request"))?;

        let body = match self.body.as_ref() {
            Some(ReqBody::Body(body)) => Some(body.try_clone(py)?),
            Some(ReqBody::PyBody(py_body)) => Some(Python::attach(|py| py_body.get().try_clone(py))?),
            None => None,
        };

        let headers = match self.headers.as_ref() {
            Some(ReqHeaders::Headers(h)) => Some(ReqHeaders::Headers(h.try_clone()?)),
            Some(ReqHeaders::PyHeaders(py_headers)) => Some(ReqHeaders::Headers(py_headers.get().try_clone()?)),
            None => None,
        };

        Ok(Request {
            inner: Some(new_inner),
            spawner: self.spawner.clone(),
            body: body.map(ReqBody::Body),
            headers,
            extensions: self.extensions.as_ref().map(|ext| ext.copy(py)).transpose()?,
            middlewares_next: self
                .middlewares_next
                .as_ref()
                .map(|next| next.clone_ref(py))
                .transpose()?,
            error_for_status: self.error_for_status,
            body_consume_config: self.body_consume_config,
        })
    }

    pub fn body_consume_config(&self) -> &BodyConsumeConfig {
        &self.body_consume_config
    }

    pub fn body_consume_config_mut(&mut self) -> &mut BodyConsumeConfig {
        &mut self.body_consume_config
    }

    pub fn inner_from_request_and_body(
        py: Python,
        request: Bound<PyAny>,
        body: Option<Bound<RequestBody>>,
    ) -> PyResult<Self> {
        let this = request.downcast::<Request>()?.try_borrow()?;
        let new_inner = this
            .inner_ref()?
            .try_clone()
            .ok_or_else(|| PyRuntimeError::new_err("Failed to clone request"))?;

        let body = body.map(|b| b.get().take_inner()).transpose()?;

        let headers = match this.headers.as_ref() {
            Some(ReqHeaders::Headers(h)) => Some(ReqHeaders::Headers(h.try_clone()?)),
            Some(ReqHeaders::PyHeaders(py_headers)) => Some(ReqHeaders::Headers(py_headers.get().try_clone()?)),
            None => None,
        };

        Ok(Request {
            inner: Some(new_inner),
            spawner: this.spawner.clone(),
            body: body.map(ReqBody::Body),
            headers,
            extensions: this.extensions.as_ref().map(|ext| ext.copy(py)).transpose()?,
            middlewares_next: this
                .middlewares_next
                .as_ref()
                .map(|next| next.clone_ref(py))
                .transpose()?,
            error_for_status: this.error_for_status,
            body_consume_config: this.body_consume_config,
        })
    }

    pub fn repr(&self, py: Python, hide_sensitive: bool) -> PyResult<String> {
        pub fn disp_repr<T: Display>(py: Python, val: T) -> PyResult<String> {
            Ok(PyString::new(py, &format!("{}", val)).repr()?.to_str()?.to_string())
        }

        let inner = self.inner_ref()?;
        let mut url = Url::from(inner.url().clone());
        let mut key_url = "url";
        if hide_sensitive {
            key_url = "origin_path";
            url = url.with_query_string(None);
        };

        let headers_dict = HeaderMap::dict_multi_value_inner(inner.headers(), py, hide_sensitive)?;
        let body_repr = match &self.body {
            Some(ReqBody::Body(body)) => body.__repr__(py)?,
            Some(ReqBody::PyBody(py_body)) => py_body.try_borrow(py)?.__repr__(py)?,
            None => "None".to_string(),
        };

        Ok(format!(
            "Request(method={}, {}={}, headers={}, body={})",
            disp_repr(py, inner.method())?,
            key_url,
            disp_repr(py, url.as_str())?,
            headers_dict.repr()?.to_str()?,
            body_repr
        ))
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
}

enum ReqHeaders {
    Headers(HeaderMap),
    PyHeaders(Py<HeaderMap>), // In Python heap
}

enum ReqBody {
    Body(RequestBody),
    PyBody(Py<RequestBody>), // In Python heap
}
