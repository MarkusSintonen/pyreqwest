use crate::allow_threads::AllowThreads;
use crate::client::internal::{SpawnRequestData, Spawner};
use crate::http::internal::json::JsonHandler;
use crate::http::internal::types::{Extensions, Method};
use crate::http::{HeaderMap, RequestBody, Url, UrlType};
use crate::middleware::{Next, NextInner, SyncNext};
use crate::response::BaseResponse;
use crate::response::internal::BodyConsumeConfig;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString, PyType};
use pyo3::{PyTraverseError, PyVisit, intern};
use std::fmt::Display;

#[pyclass(subclass)]
pub struct Request(Option<Inner>);
struct Inner {
    reqwest: reqwest::Request,
    spawner: Spawner,
    body: Option<ReqBody>,
    headers: Option<ReqHeaders>,
    extensions: Option<Extensions>,
    middlewares_next: Option<NextInner>,
    json_handler: Option<JsonHandler>,
    error_for_status: bool,
    body_consume_config: BodyConsumeConfig,
}

#[pymethods]
impl Request {
    #[getter]
    fn get_method(&self) -> PyResult<Method> {
        Ok(self.ref_inner()?.reqwest.method().clone().into())
    }

    #[setter]
    fn set_method(&mut self, value: Method) -> PyResult<()> {
        *self.mut_inner()?.reqwest.method_mut() = value.0;
        Ok(())
    }

    #[getter]
    fn get_url(&self) -> PyResult<Url> {
        Ok(self.ref_inner()?.reqwest.url().clone().into())
    }

    #[setter]
    fn set_url(&mut self, value: UrlType) -> PyResult<()> {
        *self.mut_inner()?.reqwest.url_mut() = value.0;
        Ok(())
    }

    #[getter]
    fn get_headers(&mut self, py: Python) -> PyResult<Py<HeaderMap>> {
        let inner = self.mut_inner()?;
        if inner.headers.is_none() {
            let headers = HeaderMap::from(inner.reqwest.headers().clone());
            inner.headers = Some(ReqHeaders::PyHeaders(Py::new(py, headers)?));
        }
        if let Some(ReqHeaders::Headers(h)) = &inner.headers {
            let py_headers = Py::new(py, HeaderMap::from(h.try_take_inner()?))?;
            inner.headers = Some(ReqHeaders::PyHeaders(py_headers.clone_ref(py)));
        }
        match inner.headers.as_ref() {
            Some(ReqHeaders::PyHeaders(h)) => Ok(h.clone_ref(py)),
            _ => unreachable!(),
        }
    }

    #[setter]
    fn set_headers(&mut self, py: Python, value: HeaderMap) -> PyResult<()> {
        self.mut_inner()?.headers = Some(ReqHeaders::PyHeaders(Py::new(py, value)?));
        Ok(())
    }

    #[getter]
    fn get_body(&mut self, py: Python) -> PyResult<Option<Py<RequestBody>>> {
        let inner = self.mut_inner()?;
        match inner.body.as_mut() {
            Some(ReqBody::Body(body)) => {
                let py_body = Py::new(py, body.take_inner()?)?;
                inner.body = Some(ReqBody::PyBody(py_body.clone_ref(py)));
                Ok(Some(py_body))
            }
            Some(ReqBody::PyBody(py_body)) => Ok(Some(py_body.clone_ref(py))),
            None => Ok(None),
        }
    }

    #[setter]
    fn set_body(&mut self, body: Option<Py<RequestBody>>) -> PyResult<()> {
        self.mut_inner()?.body = body.map(ReqBody::PyBody);
        Ok(())
    }

    #[getter]
    fn get_extensions(&mut self, py: Python) -> PyResult<Py<PyDict>> {
        let inner = self.mut_inner()?;
        if inner.extensions.is_none() {
            inner.extensions = Some(Extensions(PyDict::new(py).unbind()));
        }
        Ok(inner.extensions.as_ref().unwrap().0.clone_ref(py))
    }

    #[setter]
    fn set_extensions(&mut self, value: Extensions) -> PyResult<()> {
        self.mut_inner()?.extensions = Some(value);
        Ok(())
    }

    fn copy(slf: Bound<Self>) -> PyResult<Bound<PyAny>> {
        slf.call_method0(intern!(slf.py(), "__copy__"))
    }

    fn __copy__(&self, _py: Python) -> PyResult<Self> {
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
        match self.ref_inner()?.body_consume_config {
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
        let Some(inner) = self.0.as_ref() else { return Ok(()) };

        if let Some(ReqHeaders::PyHeaders(py_headers)) = &inner.headers {
            visit.call(py_headers)?;
        }
        if let Some(extensions) = &inner.extensions {
            visit.call(&extensions.0)?;
        }
        if let Some(middlewares_next) = &inner.middlewares_next {
            middlewares_next.__traverse__(&visit)?;
        }
        match &inner.body {
            Some(ReqBody::Body(body)) => body.__traverse__(visit)?,
            Some(ReqBody::PyBody(py_body)) => visit.call(py_body)?,
            None => {}
        }
        Ok(())
    }

    fn __clear__(&mut self) {
        self.0.take();
    }
}
impl Request {
    pub fn new(
        request: reqwest::Request,
        spawner: Spawner,
        body: Option<RequestBody>,
        extensions: Option<Extensions>,
        middlewares_next: Option<NextInner>,
        json_handler: Option<JsonHandler>,
        error_for_status: bool,
        body_consume_config: BodyConsumeConfig,
    ) -> Self {
        Request(Some(Inner {
            reqwest: request,
            spawner,
            extensions,
            body: body.map(ReqBody::Body),
            headers: None,
            middlewares_next,
            json_handler,
            error_for_status,
            body_consume_config,
        }))
    }

    pub async fn send_inner(py_request: &Py<PyAny>, cancel: CancelHandle) -> PyResult<BaseResponse> {
        let middlewares_next = Python::attach(|py| -> PyResult<_> {
            let mut req = py_request.bind(py).downcast::<Self>()?.try_borrow_mut()?;
            let inner = req.mut_inner()?;
            inner.body_set_task_local()?;
            inner.middlewares_next.take().map(|v| Next::new(v, py)).transpose()
        })?;

        match middlewares_next {
            Some(middlewares_next) => AllowThreads(middlewares_next.run_inner(py_request, cancel)).await,
            None => Self::spawn_request(py_request, cancel).await,
        }?
        .check_error_for_status()
    }

    pub fn blocking_send_inner(py_request: &Py<PyAny>) -> PyResult<BaseResponse> {
        let middlewares_next = Python::attach(|py| {
            let mut req = py_request.bind(py).downcast::<Self>()?.try_borrow_mut()?;
            let inner = req.mut_inner()?;
            inner.body_set_task_local()?;
            inner.middlewares_next.take().map(SyncNext::new).transpose()
        })?;

        match middlewares_next {
            Some(middlewares_next) => Python::attach(|py| middlewares_next.run_inner(py_request.bind(py))),
            None => Self::blocking_spawn_request(py_request),
        }?
        .check_error_for_status()
    }

    pub async fn spawn_request(request: &Py<PyAny>, cancel: CancelHandle) -> PyResult<BaseResponse> {
        Spawner::spawn_reqwest(Self::prepare_spawn_request(request, false)?, cancel).await
    }

    pub fn blocking_spawn_request(request: &Py<PyAny>) -> PyResult<BaseResponse> {
        Spawner::blocking_spawn_reqwest(Self::prepare_spawn_request(request, true)?)
    }

    fn prepare_spawn_request(py_request: &Py<PyAny>, is_blocking: bool) -> PyResult<SpawnRequestData> {
        let mut this = Python::attach(|py| -> PyResult<_> {
            py_request.bind(py).downcast::<Self>()?.try_borrow_mut()?.take_inner()
        })?;
        let request = &mut this.reqwest;

        match this.headers.take() {
            Some(ReqHeaders::Headers(h)) => *request.headers_mut() = h.try_take_inner()?,
            Some(ReqHeaders::PyHeaders(py_headers)) => *request.headers_mut() = py_headers.get().try_take_inner()?,
            None => {}
        }

        match this.body.take() {
            Some(ReqBody::Body(body)) => {
                body.set_task_local()?;
                *request.body_mut() = Some(body.into_reqwest(is_blocking)?)
            }
            Some(ReqBody::PyBody(py_body)) => {
                let py_body = py_body.get();
                py_body.set_task_local()?;
                *request.body_mut() = Some(py_body.into_reqwest(is_blocking)?)
            }
            None => {}
        }

        Ok(SpawnRequestData {
            request: this.reqwest,
            spawner: this.spawner.clone(),
            extensions: this.extensions.take(),
            body_consume_config: this.body_consume_config,
            json_handler: this.json_handler.take(),
            error_for_status: this.error_for_status,
        })
    }

    pub fn try_clone_inner(&self, py: Python) -> PyResult<Self> {
        py.detach(|| {
            let inner = self.ref_inner()?;
            let new_req = inner
                .reqwest
                .try_clone()
                .ok_or_else(|| PyRuntimeError::new_err("Failed to clone request"))?;

            let body = match inner.body.as_ref() {
                Some(ReqBody::Body(body)) => Some(body.try_clone()?),
                Some(ReqBody::PyBody(py_body)) => Some(py_body.get().try_clone()?),
                None => None,
            };

            let headers = match inner.headers.as_ref() {
                Some(ReqHeaders::Headers(h)) => Some(ReqHeaders::Headers(h.try_clone()?)),
                Some(ReqHeaders::PyHeaders(py_headers)) => Some(ReqHeaders::Headers(py_headers.get().try_clone()?)),
                None => None,
            };

            Ok(Request(Some(Inner {
                reqwest: new_req,
                spawner: inner.spawner.clone(),
                body: body.map(ReqBody::Body),
                headers,
                extensions: inner.extensions.as_ref().map(|ext| ext.copy()).transpose()?,
                middlewares_next: inner.middlewares_next.as_ref().map(|next| next.clone_ref()),
                json_handler: inner
                    .json_handler
                    .as_ref()
                    .map(|v| Python::attach(|py| v.clone_ref(py))),
                error_for_status: inner.error_for_status,
                body_consume_config: inner.body_consume_config,
            })))
        })
    }

    pub fn inner_from_request_and_body(request: Bound<PyAny>, body: Option<Bound<RequestBody>>) -> PyResult<Self> {
        let request = request.downcast::<Request>()?.try_borrow()?;
        let inner = request.ref_inner()?;
        let body = body.map(|b| b.get().take_inner()).transpose()?;

        let new_inner = inner
            .reqwest
            .try_clone()
            .ok_or_else(|| PyRuntimeError::new_err("Failed to clone request"))?;

        let headers = match inner.headers.as_ref() {
            Some(ReqHeaders::Headers(h)) => Some(ReqHeaders::Headers(h.try_clone()?)),
            Some(ReqHeaders::PyHeaders(py_headers)) => Some(ReqHeaders::Headers(py_headers.get().try_clone()?)),
            None => None,
        };

        Ok(Request(Some(Inner {
            reqwest: new_inner,
            spawner: inner.spawner.clone(),
            body: body.map(ReqBody::Body),
            headers,
            extensions: inner.extensions.as_ref().map(|ext| ext.copy()).transpose()?,
            middlewares_next: inner.middlewares_next.as_ref().map(|next| next.clone_ref()),
            json_handler: inner.json_handler.as_ref().map(|v| v.clone_ref(request.py())),
            error_for_status: inner.error_for_status,
            body_consume_config: inner.body_consume_config,
        })))
    }

    pub fn repr(&self, py: Python, hide_sensitive: bool) -> PyResult<String> {
        pub fn disp_repr<T: Display>(py: Python, val: T) -> PyResult<String> {
            Ok(PyString::new(py, &format!("{}", val)).repr()?.to_str()?.to_string())
        }

        let inner = self.ref_inner()?;
        let mut url = Url::from(inner.reqwest.url().clone());
        let mut key_url = "url";
        if hide_sensitive {
            key_url = "origin_path";
            url = url.with_query_string(None);
        };

        let headers_dict = HeaderMap::dict_multi_value_inner(inner.reqwest.headers(), py, hide_sensitive)?;
        let body_repr = match &inner.body {
            Some(ReqBody::Body(body)) => body.__repr__(py)?,
            Some(ReqBody::PyBody(py_body)) => py_body.try_borrow(py)?.__repr__(py)?,
            None => "None".to_string(),
        };

        Ok(format!(
            "Request(method={}, {}={}, headers={}, body={})",
            disp_repr(py, inner.reqwest.method())?,
            key_url,
            disp_repr(py, url.as_str())?,
            headers_dict.repr()?.to_str()?,
            body_repr
        ))
    }

    fn take_inner(&mut self) -> PyResult<Inner> {
        self.0
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))
    }

    fn ref_inner(&self) -> PyResult<&Inner> {
        self.0
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))
    }

    fn mut_inner(&mut self) -> PyResult<&mut Inner> {
        self.0
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))
    }
}

impl Inner {
    fn body_set_task_local(&self) -> PyResult<()> {
        match self.body.as_ref() {
            Some(ReqBody::Body(body)) => body.set_task_local(),
            Some(ReqBody::PyBody(py_body)) => py_body.get().set_task_local(),
            None => Ok(()),
        }
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
