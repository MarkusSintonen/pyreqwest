use crate::client::Spawner;
use crate::exceptions::BuilderError;
use crate::http::{Extensions, FormParams, HeaderMap, HeaderName, HeaderValue, JsonValue, QueryParams, RequestBody};
use crate::middleware::NextInner;
use crate::multipart::Form;
use crate::request::Request;
use crate::request::consumed_request::{BlockingConsumedRequest, ConsumedRequest};
use crate::request::stream_request::{BlockingStreamRequest, StreamRequest};
use crate::response::{BodyConsumeConfig, DEFAULT_READ_BUFFER_LIMIT, StreamedReadConfig};
use bytes::Bytes;
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
    error_for_status: bool,
    streamed_read_buffer_limit: Option<usize>,
}

#[pyclass(extends=BaseRequestBuilder)]
pub struct RequestBuilder;

#[pyclass(extends=BaseRequestBuilder)]
pub struct BlockingRequestBuilder;

#[pymethods]
impl RequestBuilder {
    fn build_consumed(mut slf: PyRefMut<Self>, py: Python) -> PyResult<Py<ConsumedRequest>> {
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
impl BlockingRequestBuilder {
    fn build_consumed(mut slf: PyRefMut<Self>, py: Python) -> PyResult<Py<BlockingConsumedRequest>> {
        let slf_super = slf.as_super();
        let body_config = slf_super.body_consume_config(false)?;
        BlockingConsumedRequest::new_py(py, slf_super.inner_build(body_config)?)
    }

    fn build_streamed(mut slf: PyRefMut<Self>, py: Python) -> PyResult<Py<BlockingStreamRequest>> {
        let slf_super = slf.as_super();
        let body_config = slf_super.body_consume_config(true)?;
        BlockingStreamRequest::new_py(py, slf_super.inner_build(body_config)?)
    }
}
impl BlockingRequestBuilder {
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

    fn body<'py>(mut slf: PyRefMut<'py, Self>, body: Option<Bound<RequestBody>>) -> PyResult<PyRefMut<'py, Self>> {
        slf.check_inner()?;
        slf.body = body.map(|v| v.get().take_inner()).transpose()?;
        Ok(slf)
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

    fn body_json(mut slf: PyRefMut<'_, Self>, data: JsonValue) -> PyResult<PyRefMut<'_, Self>> {
        slf.check_inner()?;
        let bytes = slf
            .py()
            .detach(|| serde_json::to_vec(&data).map_err(|e| PyValueError::new_err(e.to_string())))?;
        slf.body = Some(RequestBody::from(Bytes::from(bytes)));
        Self::apply(slf, |builder| Ok(builder.header("content-type", "application/json")))
    }

    fn body_stream<'py>(mut slf: PyRefMut<'py, Self>, stream: Bound<'py, PyAny>) -> PyResult<PyRefMut<'py, Self>> {
        slf.check_inner()?;
        slf.body = Some(RequestBody::from_stream(stream)?);
        Ok(slf)
    }

    fn timeout(slf: PyRefMut<Self>, timeout: Duration) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.timeout(timeout)))
    }

    fn multipart<'py>(slf: PyRefMut<'py, Self>, multipart: Bound<'_, Form>) -> PyResult<PyRefMut<'py, Self>> {
        let multipart = multipart.try_borrow_mut()?.build()?;
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

    fn streamed_read_buffer_limit(mut slf: PyRefMut<'_, Self>, value: usize) -> PyResult<PyRefMut<'_, Self>> {
        slf.check_inner()?;
        slf.streamed_read_buffer_limit = Some(value);
        Ok(slf)
    }

    #[staticmethod]
    fn default_streamed_read_buffer_limit() -> usize {
        DEFAULT_READ_BUFFER_LIMIT
    }

    fn _set_interceptor<'py>(
        mut slf: PyRefMut<'py, Self>,
        interceptor: Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Some(middlewares_next) = slf.middlewares_next.as_mut() {
            middlewares_next.add_middleware(interceptor)?;
        } else {
            let middlewares = Arc::new(vec![interceptor.unbind()]);
            slf.middlewares_next = Some(NextInner::new(middlewares)?);
        }
        Ok(slf)
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Some(extensions) = &self.extensions {
            visit.call(&extensions.0)?;
        }
        if let Some(middlewares_next) = &self.middlewares_next {
            middlewares_next.__traverse__(&visit)?;
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
    }
}
impl BaseRequestBuilder {
    pub fn new(
        inner: reqwest::RequestBuilder,
        spawner: Spawner,
        middlewares_next: Option<NextInner>,
        error_for_status: bool,
    ) -> Self {
        BaseRequestBuilder {
            inner: Some(inner),
            spawner: Some(spawner),
            body: None,
            extensions: None,
            middlewares_next,
            error_for_status,
            streamed_read_buffer_limit: None,
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

        let request = Request::new(
            request,
            self.spawner
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))?,
            self.body.take(),
            self.extensions.take(),
            self.middlewares_next.take(),
            self.error_for_status,
            consume_body,
        );
        Ok(request)
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
            if self.streamed_read_buffer_limit.is_some() {
                return Err(BuilderError::from_causes(
                    "Can not set streamed_read_buffer_limit when building a fully consumed request",
                    vec![],
                ));
            }
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
