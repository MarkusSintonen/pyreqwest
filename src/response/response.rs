use crate::allow_threads::AllowThreads;
use crate::asyncio::{TaskLocal, py_coro_waiter};
use crate::client::RuntimeHandle;
use crate::exceptions::{JSONDecodeError, RequestError, StatusError};
use crate::http::{HeaderMap, Mime};
use crate::internal::json::{JsonHandler, JsonLoadsContext};
use crate::internal::types::{Extensions, HeaderValue, JsonValue, StatusCode, Version};
use crate::response::SyncResponseBodyReader;
use crate::response::internal::{BodyConsumeConfig, BodyReader};
use crate::response::response_body_reader::{BaseResponseBodyReader, ResponseBodyReader};
use bytes::Bytes;
use encoding_rs::{Encoding, UTF_8};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::{PyTraverseError, PyVisit};
use pyo3_bytes::PyBytes;
use serde_json::json;
use tokio::sync::OwnedSemaphorePermit;

#[pyclass(subclass)]
pub struct BaseResponse(Option<Inner>);
pub struct Inner {
    status: StatusCode,
    version: Version,
    headers: RespHeaders,
    extensions: RespExtensions,
    body_reader: Option<RespReader>,
    runtime: RuntimeHandle,
    json_handler: Option<JsonHandler>,
    error_for_status: bool,
}

#[pyclass(extends=BaseResponse)]
pub struct Response;
#[pyclass(extends=BaseResponse)]
pub struct SyncResponse;

#[pymethods]
impl BaseResponse {
    #[getter]
    fn get_status(&self) -> PyResult<StatusCode> {
        Ok(self.ref_inner()?.status.0.into())
    }

    #[setter]
    fn set_status(&mut self, status: StatusCode) -> PyResult<()> {
        self.mut_inner()?.status = status;
        Ok(())
    }

    #[getter]
    fn get_version(&self) -> PyResult<Version> {
        Ok(self.ref_inner()?.version.clone())
    }

    #[setter]
    fn set_version(&mut self, version: Version) -> PyResult<()> {
        self.mut_inner()?.version = version;
        Ok(())
    }

    #[getter]
    fn get_headers(&mut self, py: Python) -> PyResult<Py<HeaderMap>> {
        let inner = self.mut_inner()?;
        if let RespHeaders::Headers(headers) = &inner.headers {
            let py_headers = Py::new(py, HeaderMap::from(headers.try_take_inner()?))?;
            inner.headers = RespHeaders::PyHeaders(py_headers);
        }
        match &inner.headers {
            RespHeaders::PyHeaders(py_headers) => Ok(py_headers.clone_ref(py)),
            RespHeaders::Headers(_) => unreachable!("Expected PyHeaders"),
        }
    }

    #[setter]
    fn set_headers(&mut self, value: HeaderMap) -> PyResult<()> {
        self.mut_inner()?.headers = RespHeaders::Headers(value);
        Ok(())
    }

    #[getter]
    fn get_extensions(&mut self, py: Python) -> PyResult<Py<PyDict>> {
        let inner = self.mut_inner()?;
        if let RespExtensions::Extensions(ext) = &mut inner.extensions {
            let py_ext = ext
                .remove::<Extensions>()
                .unwrap_or_else(|| Extensions(PyDict::new(py).unbind()))
                .0;
            inner.extensions = RespExtensions::PyExtensions(py_ext);
        }
        match &inner.extensions {
            RespExtensions::PyExtensions(py_ext) => Ok(py_ext.clone_ref(py)),
            RespExtensions::Extensions(_) => unreachable!("Expected PyExtensions"),
        }
    }

    #[setter]
    fn set_extensions(&mut self, extensions: Extensions) -> PyResult<()> {
        self.mut_inner()?.extensions = RespExtensions::PyExtensions(extensions.0);
        Ok(())
    }

    fn error_for_status(&self) -> PyResult<()> {
        let inner = self.ref_inner()?;
        if inner.status.0.is_success() {
            return Ok(());
        }
        let msg = if inner.status.0.is_client_error() {
            "HTTP status client error"
        } else {
            debug_assert!(inner.status.0.is_server_error());
            "HTTP status server error"
        };
        Err(StatusError::from_custom(msg, json!({"status": inner.status.0.as_u16()})))
    }

    fn get_header(&self, py: Python, name: &str) -> PyResult<Option<HeaderValue>> {
        py.detach(|| self.get_header_inner(name))
    }

    fn get_header_all(&self, py: Python, name: &str) -> PyResult<Vec<HeaderValue>> {
        py.detach(|| self.get_header_all_inner(name))
    }

    fn content_type_mime(&self, py: Python) -> PyResult<Option<Mime>> {
        py.detach(|| self.content_type_mime_inner())
    }

    async fn bytes(&mut self, #[pyo3(cancel_handle)] mut cancel: CancelHandle) -> PyResult<PyBytes> {
        AllowThreads(async { self.bytes_inner(&mut cancel).await.map(PyBytes::new) }).await
    }

    async fn json(&mut self, #[pyo3(cancel_handle)] cancel: CancelHandle) -> PyResult<Py<PyAny>> {
        if self.ref_inner()?.json_handler.as_ref().is_some_and(|v| v.has_loads()) {
            let coro = Python::attach(|py| {
                let task_local = TaskLocal::current(py)?;
                let ctx = JsonLoadsContext {
                    body_reader: self.get_body_reader_inner(py, false)?,
                    headers: self.get_headers(py)?,
                    extensions: self.get_extensions(py)?,
                };
                let coro = self
                    .ref_inner()?
                    .json_handler
                    .as_ref()
                    .ok_or_else(|| PyRuntimeError::new_err("Expected json_handler"))?
                    .call_loads(py, ctx)?;
                py_coro_waiter(coro, &task_local, Some(cancel))
            })?;
            AllowThreads(coro).await
        } else {
            self.json_inner(cancel).await // AllowThreads is used inside
        }
    }

    async fn text(&mut self, #[pyo3(cancel_handle)] mut cancel: CancelHandle) -> PyResult<String> {
        self.text_inner(&mut cancel).await // AllowThreads is used inside
    }

    // :NOCOV_START
    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        let Ok(inner) = self.ref_inner() else {
            return Ok(());
        };
        if let RespHeaders::PyHeaders(py_headers) = &inner.headers {
            visit.call(py_headers)?;
        }
        if let RespExtensions::PyExtensions(py_ext) = &inner.extensions {
            visit.call(py_ext)?;
        }
        Ok(())
    } // :NOCOV_END
}
impl BaseResponse {
    pub async fn initialize(
        response: reqwest::Response,
        request_semaphore_permit: Option<OwnedSemaphorePermit>,
        consume_body: BodyConsumeConfig,
        runtime: RuntimeHandle,
        json_handler: Option<JsonHandler>,
        error_for_status: bool,
    ) -> PyResult<Self> {
        let (body_reader, head) =
            BodyReader::initialize(response, request_semaphore_permit, consume_body, runtime.clone()).await?;

        let resp = BaseResponse(Some(Inner {
            status: StatusCode(head.status),
            version: Version(head.version),
            headers: RespHeaders::Headers(HeaderMap::from(head.headers)),
            extensions: RespExtensions::Extensions(head.extensions),
            body_reader: Some(RespReader::Reader(body_reader)),
            runtime,
            json_handler,
            error_for_status,
        }));
        Ok(resp)
    }

    pub fn check_error_for_status(self) -> PyResult<Self> {
        if self.ref_inner()?.error_for_status {
            self.error_for_status()?;
        }
        Ok(self)
    }

    pub fn take_inner(&mut self) -> PyResult<BaseResponse> {
        let inner = self
            .0
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Response already consumed"))?;
        Ok(BaseResponse(Some(inner)))
    }

    pub fn take_body_reader(&mut self) -> PyResult<RespReader> {
        self.mut_inner()?
            .body_reader
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Response body reader is closed"))
    }

    fn get_header_inner(&self, name: &str) -> PyResult<Option<HeaderValue>> {
        match self.ref_inner()?.headers {
            RespHeaders::Headers(ref headers) => headers.get_one(name),
            RespHeaders::PyHeaders(ref py_headers) => py_headers.get().get_one(name),
        }
    }

    fn get_header_all_inner(&self, name: &str) -> PyResult<Vec<HeaderValue>> {
        match self.ref_inner()?.headers {
            RespHeaders::Headers(ref headers) => headers.get_all(name),
            RespHeaders::PyHeaders(ref py_headers) => py_headers.get().get_all(name),
        }
    }

    fn content_type_mime_inner(&self) -> PyResult<Option<Mime>> {
        let Some(content_type) = self.get_header_inner("content-type")? else {
            return Ok(None);
        };
        let mime = content_type
            .0
            .to_str()
            .map_err(|e| RequestError::from_err("Invalid Content-Type header", &e))?
            .parse::<mime::Mime>()
            .map_err(|e| RequestError::from_err("Failed to parse Content-Type header as MIME", &e))?;
        Ok(Some(Mime::new(mime)))
    }

    async fn bytes_inner(&mut self, cancel: &mut CancelHandle) -> PyResult<Bytes> {
        match self.mut_inner()?.body_reader.as_mut() {
            Some(RespReader::Reader(reader)) => reader.bytes(cancel).await,
            Some(RespReader::PyReader(reader)) => reader.get().bytes_inner(cancel).await,
            None => Err(PyRuntimeError::new_err("Response body reader is closed")),
        }
    }

    async fn json_inner(&mut self, mut cancel: CancelHandle) -> PyResult<Py<PyAny>> {
        let serde_val = AllowThreads(async {
            let bytes = self.bytes_inner(&mut cancel).await?;
            match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(v) => Ok(v),
                Err(e) => Err(self.json_error(&e, &mut cancel).await?),
            }
        })
        .await?;
        Python::attach(|py| Ok(JsonValue(serde_val).into_pyobject(py)?.unbind()))
    }

    async fn text_inner(&mut self, cancel: &mut CancelHandle) -> PyResult<String> {
        AllowThreads(async {
            let bytes = self.bytes_inner(cancel).await?;
            let encoding = self
                .content_type_mime_inner()?
                .and_then(|mime| mime.get_param("charset").map(String::from))
                .and_then(|charset| Encoding::for_label(charset.as_bytes()))
                .unwrap_or(UTF_8);
            let (text, _, _) = encoding.decode(&bytes);
            Ok(text.into_owned())
        })
        .await
    }

    fn get_body_reader_inner(&mut self, py: Python, is_blocking: bool) -> PyResult<Py<BaseResponseBodyReader>> {
        let inner = self.mut_inner()?;
        if inner.body_reader.is_none() {
            return Err(PyRuntimeError::new_err("Response body reader is closed"));
        };
        if let Some(RespReader::Reader(_)) = inner.body_reader.as_ref()
            && let Some(RespReader::Reader(reader)) = inner.body_reader.take()
        {
            let py_body_reader = if is_blocking {
                SyncResponseBodyReader::new_py(py, reader)?
                    .into_bound(py)
                    .cast_into::<BaseResponseBodyReader>()?
            } else {
                ResponseBodyReader::new_py(py, reader)?
                    .into_bound(py)
                    .cast_into::<BaseResponseBodyReader>()?
            };
            inner.body_reader = Some(RespReader::PyReader(py_body_reader.unbind()));
        }
        match inner.body_reader.as_ref() {
            Some(RespReader::PyReader(py_reader)) => Ok(py_reader.clone_ref(py)),
            _ => unreachable!("Expected PyReader"),
        }
    }

    async fn json_error(&mut self, e: &serde_json::error::Error, cancel: &mut CancelHandle) -> PyResult<PyErr> {
        let text = self.text_inner(cancel).await?;
        let details = json!({"pos": Self::json_error_pos(&text, e), "doc": text, "causes": serde_json::Value::Null});
        Ok(JSONDecodeError::from_custom(&e.to_string(), details))
    }

    fn json_error_pos(content: &str, e: &serde_json::error::Error) -> usize {
        let (line, column) = (e.line(), e.column());
        // Use byte position to have error case efficient
        content
            .split('\n')
            .take(line)
            .enumerate()
            .map(|(idx, s)| {
                if idx == line - 1 {
                    if column == s.len() {
                        column // Error at the end of the content
                    } else {
                        column.saturating_sub(1)
                    }
                } else {
                    s.len() + 1 // Other lines, +1 for '\n'
                }
            })
            .sum::<usize>()
    }

    fn ref_inner(&self) -> PyResult<&Inner> {
        self.0
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Response already consumed"))
    }

    fn mut_inner(&mut self) -> PyResult<&mut Inner> {
        self.0
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("Response already consumed"))
    }
}

#[pymethods]
impl Response {
    #[getter]
    fn get_body_reader(mut slf: PyRefMut<'_, Self>, py: Python) -> PyResult<Py<ResponseBodyReader>> {
        Ok(slf
            .as_super()
            .get_body_reader_inner(py, false)?
            .into_bound(py)
            .cast_into::<ResponseBodyReader>()?
            .unbind())
    }
}
impl Response {
    pub fn new_py(py: Python, inner: BaseResponse) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self))
    }
}

#[pymethods]
impl SyncResponse {
    #[getter]
    fn get_body_reader<'py>(
        mut slf: PyRefMut<'py, Self>,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, SyncResponseBodyReader>> {
        Ok(slf
            .as_super()
            .get_body_reader_inner(py, true)?
            .into_bound(py)
            .cast_into::<SyncResponseBodyReader>()?)
    }

    fn bytes(slf: PyRefMut<Self>) -> PyResult<PyBytes> {
        Self::runtime(slf.as_ref())?.blocking_spawn(slf.into_super().bytes(CancelHandle::new()))
    }

    fn json(mut slf: PyRefMut<Self>, py: Python) -> PyResult<Py<PyAny>> {
        let json_handler = match slf.as_super().ref_inner()?.json_handler.as_ref() {
            Some(h) if h.has_loads() => h.clone_ref(py),
            _ => {
                return Self::runtime(slf.as_ref())?.blocking_spawn(slf.into_super().json_inner(CancelHandle::new()));
            }
        };
        let ctx = JsonLoadsContext {
            headers: slf.as_super().get_headers(py)?,
            extensions: slf.as_super().get_extensions(py)?,
            body_reader: SyncResponse::get_body_reader(slf, py)?
                .cast_into::<BaseResponseBodyReader>()?
                .unbind(),
        };
        Ok(json_handler.call_loads(py, ctx)?.unbind())
    }

    fn text(slf: PyRefMut<Self>) -> PyResult<String> {
        Self::runtime(slf.as_ref())?.blocking_spawn(slf.into_super().text(CancelHandle::new()))
    }
}
impl SyncResponse {
    pub fn new_py(py: Python, inner: BaseResponse) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self))
    }

    pub fn runtime(slf: &BaseResponse) -> PyResult<RuntimeHandle> {
        Ok(slf.ref_inner()?.runtime.clone())
    }
}

pub enum RespReader {
    Reader(BodyReader),
    PyReader(Py<BaseResponseBodyReader>), // In Python heap
}
impl RespReader {
    pub async fn close(&self) {
        match self {
            RespReader::Reader(reader) => reader.close(),
            RespReader::PyReader(reader) => reader.get().close().await,
        }
    }
}

enum RespHeaders {
    Headers(HeaderMap),
    PyHeaders(Py<HeaderMap>), // In Python heap
}

enum RespExtensions {
    Extensions(http::Extensions),
    PyExtensions(Py<PyDict>), // In Python heap
}
