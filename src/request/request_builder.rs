use crate::client::internal::Spawner;
use crate::exceptions::BuilderError;
use crate::http::HeaderMap;
use crate::http::internal::json::{JsonDumpsContext, JsonHandler};
use crate::http::internal::types::{Extensions, FormParams, HeaderName, HeaderValue, JsonValue, QueryParams};
use crate::middleware::NextInner;
use crate::multipart::FormBuilder;
use crate::request::consumed_request::{ConsumedRequest, SyncConsumedRequest};
use crate::request::request::RequestData;
use crate::request::stream_request::{StreamRequest, SyncStreamRequest};
use crate::request::{Request, RequestBody};
use crate::response::internal::{BodyConsumeConfig, DEFAULT_READ_BUFFER_LIMIT, StreamedReadConfig};
use bytes::Bytes;
use http::header::CONTENT_TYPE;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::{PyTraverseError, PyVisit};
use pyo3_bytes::PyBytes;
use std::sync::Arc;
use std::time::Duration;

#[pyclass(subclass)]
pub struct BaseRequestBuilder {
    inner: Option<reqwest::RequestBuilder>,
    spawner: Option<Spawner>,
    body: Option<RequestBody>,
    extensions: Option<Extensions>,
    middlewares_next: Option<NextInner>,
    json_handler: Option<JsonHandler>,
    error_for_status: bool,
    streamed_read_buffer_limit: Option<usize>,
    is_blocking: bool,
}

#[pyclass(extends=BaseRequestBuilder)]
pub struct RequestBuilder;

#[pyclass(extends=BaseRequestBuilder)]
pub struct SyncRequestBuilder;

#[pymethods]
impl RequestBuilder {
    fn build(mut slf: PyRefMut<Self>, py: Python) -> PyResult<Py<ConsumedRequest>> {
        let slf_super = slf.as_super();
        let body_config = slf_super.body_consume_config(false)?;
        ConsumedRequest::new_py(py, slf_super.inner_build(body_config)?)
    }

    fn build_streamed(mut slf: PyRefMut<Self>, py: Python) -> PyResult<Py<StreamRequest>> {
        let slf_super = slf.as_super();
        let body_config = slf_super.body_consume_config(true)?;
        StreamRequest::new_py(py, slf_super.inner_build(body_config)?)
    }
}
impl RequestBuilder {
    pub fn new_py(py: Python, inner: BaseRequestBuilder) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self))
    }
}

#[pymethods]
impl SyncRequestBuilder {
    fn build(mut slf: PyRefMut<Self>, py: Python) -> PyResult<Py<SyncConsumedRequest>> {
        let slf_super = slf.as_super();
        let body_config = slf_super.body_consume_config(false)?;
        SyncConsumedRequest::new_py(py, slf_super.inner_build(body_config)?)
    }

    fn build_streamed(mut slf: PyRefMut<Self>, py: Python) -> PyResult<Py<SyncStreamRequest>> {
        let slf_super = slf.as_super();
        let body_config = slf_super.body_consume_config(true)?;
        SyncStreamRequest::new_py(py, slf_super.inner_build(body_config)?)
    }
}
impl SyncRequestBuilder {
    pub fn new_py(py: Python, inner: BaseRequestBuilder) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self))
    }
}

#[pymethods]
impl BaseRequestBuilder {
    fn error_for_status(mut slf: PyRefMut<Self>, value: bool) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.error_for_status = value;
        Ok(slf)
    }

    fn header(slf: PyRefMut<Self>, name: HeaderName, value: HeaderValue) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.header(name.0, value.0)))
    }

    fn headers(slf: PyRefMut<'_, Self>, headers: HeaderMap) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |builder| Ok(builder.headers(headers.try_take_inner()?)))
    }

    fn basic_auth(slf: PyRefMut<Self>, username: String, password: Option<String>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.basic_auth(username, password)))
    }

    fn bearer_auth(slf: PyRefMut<Self>, token: String) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.bearer_auth(token)))
    }

    fn body_bytes(mut slf: PyRefMut<Self>, body: PyBytes) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.body = Some(RequestBody::from_bytes(body));
        Ok(slf)
    }

    fn body_text(mut slf: PyRefMut<Self>, body: String) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.body = Some(RequestBody::from_text(body));
        Ok(slf)
    }

    fn body_json<'py>(mut slf: PyRefMut<'py, Self>, data: Py<PyAny>, py: Python) -> PyResult<PyRefMut<'py, Self>> {
        slf.check_inner()?;
        let bytes = if slf.json_handler.as_ref().is_some_and(|v| v.has_dumps()) {
            slf.json_handler
                .as_ref()
                .unwrap()
                .call_dumps(py, JsonDumpsContext { data })?
        } else {
            let json_val: JsonValue = data.bind(py).extract()?;
            slf.py().detach(|| {
                serde_json::to_vec(&json_val)
                    .map(Bytes::from)
                    .map_err(|e| PyValueError::new_err(e.to_string()))
            })?
        };
        slf.body = Some(RequestBody::from(bytes));
        Self::apply(slf, |builder| Ok(builder.header(CONTENT_TYPE, "application/json")))
    }

    fn body_stream<'py>(mut slf: PyRefMut<'py, Self>, stream: Bound<'py, PyAny>) -> PyResult<PyRefMut<'py, Self>> {
        slf.check_inner()?;
        slf.body = Some(RequestBody::from_stream(stream)?);
        Ok(slf)
    }

    fn timeout(slf: PyRefMut<Self>, timeout: Duration) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.timeout(timeout)))
    }

    fn multipart<'py>(slf: PyRefMut<'py, Self>, multipart: Bound<'_, FormBuilder>) -> PyResult<PyRefMut<'py, Self>> {
        let mut multipart = multipart.try_borrow_mut()?;
        if slf.is_blocking && multipart.is_async() {
            return Err(BuilderError::from_causes(
                "Can not use async multipart (stream) in a blocking request",
                vec![],
            ));
        }
        let multipart = multipart.build()?;
        Self::apply(slf, |builder| Ok(builder.multipart(multipart)))
    }

    fn query<'py>(slf: PyRefMut<'py, Self>, query: Bound<'_, PyAny>) -> PyResult<PyRefMut<'py, Self>> {
        let query = query.extract::<QueryParams>()?.0;
        Self::apply(slf, |builder| Ok(builder.query(&query)))
    }

    fn form<'py>(slf: PyRefMut<'py, Self>, form: Bound<'_, PyAny>) -> PyResult<PyRefMut<'py, Self>> {
        let form = form.extract::<FormParams>()?.0;
        Self::apply(slf, |builder| Ok(builder.form(&form)))
    }

    fn extensions(mut slf: PyRefMut<'_, Self>, extensions: Extensions) -> PyResult<PyRefMut<'_, Self>> {
        slf.check_inner()?;
        slf.extensions = Some(extensions.copy(slf.py())?);
        Ok(slf)
    }

    fn with_middleware<'py>(
        mut slf: PyRefMut<'py, Self>,
        middleware: Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Some(middlewares_next) = slf.middlewares_next.as_mut() {
            middlewares_next.add_middleware(middleware)?;
        } else {
            slf.middlewares_next = Some(NextInner::new(Arc::new(vec![middleware.unbind()]))?);
        }
        Ok(slf)
    }

    fn streamed_read_buffer_limit(mut slf: PyRefMut<'_, Self>, value: usize) -> PyResult<PyRefMut<'_, Self>> {
        slf.check_inner()?;
        slf.streamed_read_buffer_limit = Some(value);
        Ok(slf)
    }

    #[staticmethod]
    fn default_streamed_read_buffer_limit() -> usize {
        DEFAULT_READ_BUFFER_LIMIT
    }

    // :NOCOV_START
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Some(extensions) = &self.extensions {
            visit.call(&extensions.0)?;
        }
        if let Some(middlewares_next) = &self.middlewares_next {
            middlewares_next.__traverse__(&visit)?;
        }
        if let Some(json_handler) = &self.json_handler {
            json_handler.__traverse__(&visit)?;
        }
        if let Some(body) = &self.body {
            body.__traverse__(visit)?;
        }
        Ok(())
    }

    fn __clear__(&mut self) {
        self.inner = None;
        self.spawner = None;
        self.body = None;
        self.extensions = None;
        self.middlewares_next = None;
        self.json_handler = None;
    } // :NOCOV_END
}
impl BaseRequestBuilder {
    pub fn new(
        inner: reqwest::RequestBuilder,
        spawner: Spawner,
        middlewares_next: Option<NextInner>,
        json_handler: Option<JsonHandler>,
        error_for_status: bool,
        is_blocking: bool,
    ) -> Self {
        BaseRequestBuilder {
            inner: Some(inner),
            spawner: Some(spawner),
            body: None,
            extensions: None,
            middlewares_next,
            json_handler,
            error_for_status,
            streamed_read_buffer_limit: None,
            is_blocking,
        }
    }

    fn inner_build(&mut self, consume_body: BodyConsumeConfig) -> PyResult<Request> {
        let request = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))?
            .build()
            .map_err(|e| BuilderError::from_err("Failed to build request", &e))?;

        if request.body().is_some() && self.body.is_some() {
            return Err(BuilderError::from_causes("Can not set body when multipart or form is used", vec![]));
        }

        let request_data = RequestData {
            spawner: self
                .spawner
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))?,
            reqwest: request,
            extensions: self.extensions.take(),
            body_consume_config: consume_body,
            json_handler: self.json_handler.take(),
            error_for_status: self.error_for_status,
        };
        Ok(Request::new(request_data, self.body.take(), self.middlewares_next.take()))
    }

    fn body_consume_config(&self, is_streamed: bool) -> PyResult<BodyConsumeConfig> {
        if is_streamed {
            let config = StreamedReadConfig {
                read_buffer_limit: self
                    .streamed_read_buffer_limit
                    .unwrap_or(BaseRequestBuilder::default_streamed_read_buffer_limit()),
            };
            Ok(BodyConsumeConfig::Streamed(config))
        } else {
            Ok(BodyConsumeConfig::FullyConsumed)
        }
    }

    pub fn inner_timeout(&mut self, timeout: Duration) -> PyResult<&mut Self> {
        self.apply_inner(|b| Ok(b.timeout(timeout)))
    }

    pub fn inner_headers(&mut self, headers: &HeaderMap) -> PyResult<&mut Self> {
        self.apply_inner(|b| Ok(b.headers(headers.try_clone_inner()?)))
    }

    fn check_inner(&self) -> PyResult<()> {
        self.inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))
            .map(|_| ())
    }

    fn apply<F>(mut slf: PyRefMut<Self>, fun: F) -> PyResult<PyRefMut<Self>>
    where
        F: FnOnce(reqwest::RequestBuilder) -> PyResult<reqwest::RequestBuilder>,
        F: Send,
    {
        let builder = slf
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))?;
        slf.inner = Some(slf.py().detach(|| fun(builder))?);
        Ok(slf)
    }

    fn apply_inner<F>(&mut self, fun: F) -> PyResult<&mut Self>
    where
        F: FnOnce(reqwest::RequestBuilder) -> PyResult<reqwest::RequestBuilder>,
    {
        let builder = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))?;
        self.inner = Some(fun(builder)?);
        Ok(self)
    }
}
