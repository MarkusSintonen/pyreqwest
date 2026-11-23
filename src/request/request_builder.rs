use crate::client::internal::Spawner;
use crate::client::{BaseClient, BaseClientBuilder};
use crate::exceptions::BuilderError;
use crate::http::HeaderMap;
use crate::internal::allow_threads::AllowThreads;
use crate::internal::json::{JsonDumpsContext, JsonHandler};
use crate::internal::types::{Extensions, FormParams, HeaderName, HeaderValue, JsonValue, Method, QueryParams};
use crate::internal::utils::{basic_auth, bearer_auth};
use crate::middleware::NextInner;
use crate::multipart::FormBuilder;
use crate::request::consumed_request::{ConsumedRequest, SyncConsumedRequest};
use crate::request::internal::RequestBuilderHeaders;
use crate::request::request::RequestData;
use crate::request::stream_request::{StreamRequest, SyncStreamRequest};
use crate::request::{Request, RequestBody};
use crate::response::internal::{BodyConsumeConfig, DEFAULT_READ_BUFFER_LIMIT, StreamedReadConfig};
use crate::response::{Response, SyncResponse};
use bytes::Bytes;
use http::header::{AUTHORIZATION, CONTENT_TYPE};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::{PyTraverseError, PyVisit};
use pyo3_bytes::PyBytes;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

#[pyclass(subclass)]
pub struct BaseRequestBuilder {
    inner: Option<reqwest::RequestBuilder>,
    spawner: Option<Spawner>,
    headers: RequestBuilderHeaders,
    body: Option<Body>,
    extensions: Option<Extensions>,
    middlewares_next: Option<NextInner>,
    json_handler: Option<JsonHandler>,
    error_for_status: bool,
    connection_verbose: bool,
    streamed_read_buffer_limit: Option<usize>,
    is_blocking: bool,
}

#[pyclass(extends=BaseRequestBuilder, subclass)]
pub struct RequestBuilder;

#[pyclass(extends=BaseRequestBuilder)]
pub struct OneOffRequestBuilder(Option<BaseClient>);

#[pyclass(extends=BaseRequestBuilder, subclass)]
pub struct SyncRequestBuilder;

#[pyclass(extends=BaseRequestBuilder)]
pub struct SyncOneOffRequestBuilder(Option<BaseClient>);

#[pymethods]
impl RequestBuilder {
    fn body_stream<'py>(mut slf: PyRefMut<'py, Self>, stream: Bound<'py, PyAny>) -> PyResult<PyRefMut<'py, Self>> {
        slf.as_super().inner_body_stream(stream)?;
        Ok(slf)
    }

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

    fn with_middleware<'py>(
        mut slf: PyRefMut<'py, Self>,
        middleware: Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        BaseRequestBuilder::inner_with_middleware(slf.as_mut(), middleware)?;
        Ok(slf)
    }
}
impl RequestBuilder {
    pub fn new_py(py: Python, inner: BaseRequestBuilder) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self))
    }
}

#[pymethods]
impl SyncRequestBuilder {
    fn body_stream<'py>(mut slf: PyRefMut<'py, Self>, stream: Bound<'py, PyAny>) -> PyResult<PyRefMut<'py, Self>> {
        slf.as_super().inner_body_stream(stream)?;
        Ok(slf)
    }

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

    fn with_middleware<'py>(
        mut slf: PyRefMut<'py, Self>,
        middleware: Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        BaseRequestBuilder::inner_with_middleware(slf.as_mut(), middleware)?;
        Ok(slf)
    }
}
impl SyncRequestBuilder {
    pub fn new_py(py: Python, inner: BaseRequestBuilder) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self))
    }
}

#[pymethods]
impl OneOffRequestBuilder {
    async fn send(slf: Py<Self>, #[pyo3(cancel_handle)] cancel: CancelHandle) -> PyResult<Py<Response>> {
        let (_client, consumed_request) = Python::attach(|py| -> PyResult<_> {
            let mut slf_ref = slf.bind(py).try_borrow_mut()?;
            let client = slf_ref
                .0
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?;
            let request = slf_ref.as_super().inner_build(BodyConsumeConfig::FullyConsumed)?;
            Ok((client, ConsumedRequest::new_py(py, request)?))
        })?;

        let resp = AllowThreads(Request::send_inner(consumed_request.as_any(), cancel)).await?;
        Python::attach(|py| Response::new_py(py, resp))
    }

    fn with_middleware<'py>(
        mut slf: PyRefMut<'py, Self>,
        middleware: Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        BaseRequestBuilder::inner_with_middleware(slf.as_mut(), middleware)?;
        Ok(slf)
    }
}
impl OneOffRequestBuilder {
    fn new_py(py: Python, inner: BaseRequestBuilder, client: BaseClient) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self(Some(client))))
    }

    pub fn new(py: Python, method: Method, url: Bound<PyAny>) -> PyResult<Py<Self>> {
        let client = BaseClientBuilder::new().build_client_base(py)?;
        let request_builder = client.create_async_request_builder(method, url)?;
        Self::new_py(py, request_builder, client)
    }
}

#[pymethods]
impl SyncOneOffRequestBuilder {
    fn send(mut slf: PyRefMut<Self>, py: Python) -> PyResult<Py<SyncResponse>> {
        let _client = slf
            .0
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?;
        let request = slf.as_super().inner_build(BodyConsumeConfig::FullyConsumed)?;
        let consumed_request = SyncConsumedRequest::new_py(py, request)?;

        let resp = Request::blocking_send_inner(consumed_request.bind(py).as_super())?;
        SyncResponse::new_py(py, resp)
    }

    fn with_middleware<'py>(
        mut slf: PyRefMut<'py, Self>,
        middleware: Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        BaseRequestBuilder::inner_with_middleware(slf.as_mut(), middleware)?;
        Ok(slf)
    }
}
impl SyncOneOffRequestBuilder {
    fn new_py(py: Python, inner: BaseRequestBuilder, client: BaseClient) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self(Some(client))))
    }

    pub fn new(py: Python, method: Method, url: Bound<PyAny>) -> PyResult<Py<Self>> {
        let client = BaseClientBuilder::new().build_client_base(py)?;
        let request_builder = client.create_sync_request_builder(method, url)?;
        Self::new_py(py, request_builder, client)
    }
}

#[pymethods]
impl BaseRequestBuilder {
    #[pyo3(signature = (enable=true))]
    fn error_for_status(mut slf: PyRefMut<Self>, enable: bool) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.error_for_status = enable;
        Ok(slf)
    }

    #[pyo3(signature = (key, value, *, is_sensitive=false))]
    fn header(
        mut slf: PyRefMut<Self>,
        key: HeaderName,
        value: HeaderValue,
        is_sensitive: bool,
    ) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        let mut value = value.0;
        if is_sensitive {
            value.set_sensitive(true);
        }
        slf.headers.header(key.0, value)?;
        Ok(slf)
    }

    fn headers(mut slf: PyRefMut<'_, Self>, headers: HeaderMap) -> PyResult<PyRefMut<'_, Self>> {
        slf.check_inner()?;
        slf.headers.headers(headers.try_take_inner()?)?;
        Ok(slf)
    }

    fn basic_auth(mut slf: PyRefMut<Self>, username: String, password: Option<String>) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.headers.header(AUTHORIZATION, basic_auth(username, password)?)?;
        Ok(slf)
    }

    fn bearer_auth(mut slf: PyRefMut<Self>, token: String) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.headers.header(AUTHORIZATION, bearer_auth(token)?)?;
        Ok(slf)
    }

    fn body_bytes(mut slf: PyRefMut<Self>, body: PyBytes) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.body = Some(Body::Any(RequestBody::from_bytes(body)));
        Ok(slf)
    }

    fn body_text(mut slf: PyRefMut<Self>, body: String) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.body = Some(Body::Any(RequestBody::from_text(body)));
        Ok(slf)
    }

    fn body_json<'py>(mut slf: PyRefMut<'py, Self>, body: Py<PyAny>) -> PyResult<PyRefMut<'py, Self>> {
        slf.check_inner()?;
        let bytes = if let Some(handler) = slf.json_handler.as_ref()
            && handler.has_dumps()
        {
            handler.call_dumps(slf.py(), JsonDumpsContext { data: body })?
        } else {
            let json_val: JsonValue = body.bind(slf.py()).extract()?;
            slf.py().detach(|| {
                serde_json::to_vec(&json_val)
                    .map(Bytes::from)
                    .map_err(|e| PyValueError::new_err(e.to_string()))
            })?
        };
        slf.body = Some(Body::Json(RequestBody::from(bytes)));
        Ok(slf)
    }

    fn query<'py>(slf: PyRefMut<'py, Self>, query: Bound<'_, PyAny>) -> PyResult<PyRefMut<'py, Self>> {
        let query = query.extract::<QueryParams>()?.0;
        Self::apply(slf, true, |builder| Ok(builder.query(&query)))
    }

    fn timeout(slf: PyRefMut<Self>, timeout: Duration) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, false, |builder| Ok(builder.timeout(timeout)))
    }

    fn multipart<'py>(slf: PyRefMut<'py, Self>, multipart: Bound<'_, FormBuilder>) -> PyResult<PyRefMut<'py, Self>> {
        let multipart = {
            let mut multipart = multipart.try_borrow_mut()?;
            if slf.is_blocking && multipart.is_async() {
                return Err(BuilderError::from_msg("Can not use async multipart (stream) in a blocking request"));
            }
            multipart.build()?
        };
        Self::apply(slf, true, |builder| Ok(builder.multipart(multipart)))
    }

    fn form<'py>(slf: PyRefMut<'py, Self>, form: Bound<'_, PyAny>) -> PyResult<PyRefMut<'py, Self>> {
        let form = form.extract::<FormParams>()?.0;
        Self::apply(slf, true, |builder| Ok(builder.form(&form)))
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
        match &self.body {
            Some(Body::Any(body)) => body.__traverse__(visit)?,
            Some(Body::Json(body)) => body.__traverse__(visit)?,
            None => {}
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
        default_headers: Option<http::HeaderMap>,
        connection_verbose: bool,
        is_blocking: bool,
    ) -> Self {
        BaseRequestBuilder {
            inner: Some(inner),
            spawner: Some(spawner),
            headers: RequestBuilderHeaders::new(default_headers),
            body: None,
            extensions: None,
            middlewares_next,
            json_handler,
            error_for_status,
            connection_verbose,
            streamed_read_buffer_limit: None,
            is_blocking,
        }
    }

    fn inner_body_stream<'py>(&mut self, stream: Bound<'py, PyAny>) -> PyResult<()> {
        self.check_inner()?;
        self.body = Some(Body::Any(RequestBody::from_stream(stream)?));
        Ok(())
    }

    fn inner_build(&mut self, consume_body: BodyConsumeConfig) -> PyResult<Request> {
        let mut request = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))?
            .build()
            .map_err(|e| BuilderError::from_err("Failed to build request", &e))?;

        if request.body().is_some() && self.body.is_some() {
            return Err(BuilderError::from_msg("Can not set body when multipart or form is used"));
        }

        self.headers.drain_into_headers(request.headers_mut());

        let body = match self.body.take() {
            Some(Body::Json(body)) => {
                request
                    .headers_mut()
                    .insert(CONTENT_TYPE, HeaderValue::from_str("application/json")?.0);
                Some(body)
            }
            Some(Body::Any(body)) => Some(body),
            None => None,
        };

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
            connection_verbose: self.connection_verbose,
        };
        Ok(Request::new(request_data, body, self.middlewares_next.take()))
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

    pub fn inner_with_middleware(&mut self, middleware: Bound<PyAny>) -> PyResult<()> {
        if let Some(middlewares_next) = self.middlewares_next.as_mut() {
            middlewares_next.add_middleware(middleware)?;
        } else {
            self.middlewares_next = Some(NextInner::new(Arc::new(vec![middleware.unbind()]))?);
        }
        Ok(())
    }

    fn check_inner(&self) -> PyResult<()> {
        self.inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))
            .map(|_| ())
    }

    fn apply<F>(mut slf: PyRefMut<Self>, py_detach: bool, fun: F) -> PyResult<PyRefMut<Self>>
    where
        F: FnOnce(reqwest::RequestBuilder) -> PyResult<reqwest::RequestBuilder>,
        F: Send,
    {
        let builder = slf
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already built"))?;
        if py_detach {
            slf.inner = Some(slf.py().detach(|| fun(builder))?);
        } else {
            slf.inner = Some(fun(builder)?);
        }
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

enum Body {
    Json(RequestBody),
    Any(RequestBody),
}
