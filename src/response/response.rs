use crate::allow_threads::AllowThreads;
use crate::client::Handle;
use crate::exceptions::{JSONDecodeError, RequestError, StatusError};
use crate::http::internal::types::{Extensions, HeaderValue, JsonValue, StatusCode, Version};
use crate::http::{HeaderMap, Mime};
use crate::response::internal::{BodyConsumeConfig, BodyReader, DEFAULT_READ_BUFFER_LIMIT};
use encoding_rs::{Encoding, UTF_8};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::{PyTraverseError, PyVisit};
use pyo3_bytes::PyBytes;
use serde_json::json;
use tokio::sync::OwnedSemaphorePermit;

#[pyclass(subclass)]
pub struct BaseResponse {
    #[pyo3(get, set)]
    status: StatusCode,
    #[pyo3(get, set)]
    version: Version,

    headers: Option<RespHeaders>,
    extensions: Option<RespExtensions>,

    body_reader: BodyReader,
    runtime: Option<Handle>,
}

#[pyclass(extends=BaseResponse)]
pub struct Response;
#[pyclass(extends=BaseResponse)]
pub struct BlockingResponse;

#[pymethods]
impl BaseResponse {
    #[getter]
    fn get_headers(&mut self, py: Python) -> PyResult<&Py<HeaderMap>> {
        if self.headers.is_none() {
            return Err(PyRuntimeError::new_err("Expected headers"));
        };
        if let RespHeaders::Headers(headers) = self.headers.as_mut().unwrap() {
            let py_headers = Py::new(py, HeaderMap::from(headers.try_take_inner()?))?;
            self.headers = Some(RespHeaders::PyHeaders(py_headers));
        }
        match self.headers.as_ref().unwrap() {
            RespHeaders::PyHeaders(py_headers) => Ok(py_headers),
            RespHeaders::Headers(_) => Err(PyRuntimeError::new_err("Expected PyHeaders")),
        }
    }

    #[setter]
    fn set_headers(&mut self, value: Py<HeaderMap>) -> PyResult<()> {
        self.headers = Some(RespHeaders::PyHeaders(value));
        Ok(())
    }

    #[getter]
    fn get_extensions(&mut self, py: Python) -> PyResult<&Py<PyDict>> {
        if self.extensions.is_none() {
            return Err(PyRuntimeError::new_err("Expected extensions"));
        };
        if let RespExtensions::Extensions(ext) = self.extensions.as_mut().unwrap() {
            let py_ext = ext
                .remove::<Extensions>()
                .unwrap_or_else(|| Extensions(PyDict::new(py).unbind()))
                .0;
            self.extensions = Some(RespExtensions::PyExtensions(py_ext));
        }
        match self.extensions.as_ref().unwrap() {
            RespExtensions::PyExtensions(py_ext) => Ok(py_ext),
            RespExtensions::Extensions(_) => Err(PyRuntimeError::new_err("Expected PyExtensions")),
        }
    }

    #[setter]
    fn set_extensions(&mut self, extensions: Extensions) {
        self.extensions = Some(RespExtensions::PyExtensions(extensions.0));
    }

    pub fn error_for_status(&self) -> PyResult<()> {
        if self.status.0.is_success() {
            return Ok(());
        }
        let msg = if self.status.0.is_client_error() {
            "HTTP status client error"
        } else {
            debug_assert!(self.status.0.is_server_error());
            "HTTP status server error"
        };
        Err(StatusError::from_custom(msg, json!({"status": self.status.0.as_u16()})))
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

    async fn bytes(&mut self) -> PyResult<PyBytes> {
        AllowThreads(async {
            let bytes = self.body_reader.read_all_once().await?;
            Ok(PyBytes::new(bytes))
        })
        .await
    }

    async fn json(&mut self) -> PyResult<JsonValue> {
        AllowThreads(async {
            let bytes = self.body_reader.read_all_once().await?;
            match serde_json::from_slice(&bytes) {
                Ok(v) => Ok(v),
                Err(e) => Err(self.json_error(&e).await?),
            }
        })
        .await
    }

    async fn text(&mut self) -> PyResult<String> {
        AllowThreads(async {
            let bytes = self.body_reader.read_all_once().await?;
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

    #[pyo3(signature = (amount=DEFAULT_READ_BUFFER_LIMIT))]
    async fn read(&mut self, amount: usize) -> PyResult<PyBytes> {
        AllowThreads(async { self.body_reader.read(amount).await.map(PyBytes::from) }).await
    }

    async fn next_chunk(&mut self) -> PyResult<Option<PyBytes>> {
        AllowThreads(async { Ok(self.body_reader.next_chunk().await?.map(PyBytes::from)) }).await
    }

    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Some(RespHeaders::PyHeaders(py_headers)) = &self.headers {
            visit.call(py_headers)?;
        }
        if let Some(RespExtensions::PyExtensions(py_ext)) = &self.extensions {
            visit.call(py_ext)?;
        }
        Ok(())
    }

    fn __clear__(&mut self) {
        self.headers = None;
        self.extensions = None;
    }
}
impl BaseResponse {
    pub async fn initialize(
        response: reqwest::Response,
        request_semaphore_permit: Option<OwnedSemaphorePermit>,
        consume_body: BodyConsumeConfig,
        runtime: Option<Handle>,
    ) -> PyResult<Self> {
        let (body_reader, head) =
            BodyReader::initialize(response, request_semaphore_permit, consume_body, runtime.clone()).await?;

        let resp = BaseResponse {
            status: StatusCode(head.status),
            version: Version(head.version),
            headers: Some(RespHeaders::Headers(HeaderMap::from(head.headers))),
            extensions: Some(RespExtensions::Extensions(head.extensions)),
            body_reader,
            runtime,
        };
        Ok(resp)
    }

    fn get_header_inner(&self, name: &str) -> PyResult<Option<HeaderValue>> {
        match self.headers {
            Some(RespHeaders::Headers(ref headers)) => headers.get_one(name),
            Some(RespHeaders::PyHeaders(ref py_headers)) => py_headers.get().get_one(name),
            None => Err(PyRuntimeError::new_err("Expected headers")),
        }
    }

    fn get_header_all_inner(&self, name: &str) -> PyResult<Vec<HeaderValue>> {
        match self.headers {
            Some(RespHeaders::Headers(ref headers)) => headers.get_all(name),
            Some(RespHeaders::PyHeaders(ref py_headers)) => py_headers.get().get_all(name),
            None => Err(PyRuntimeError::new_err("Expected headers")),
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

    pub fn inner_close(&self) {
        self.body_reader.close() // Close the receiver to stop the reader background task
    }

    async fn json_error(&mut self, e: &serde_json::error::Error) -> PyResult<PyErr> {
        let text = self.text().await?;
        let details = json!({"pos": Self::json_error_pos(&text, e), "doc": text, "causes": serde_json::Value::Null});
        Ok(JSONDecodeError::from_custom(&e.to_string(), details))
    }

    fn json_error_pos(content: &str, e: &serde_json::error::Error) -> usize {
        let (line, column) = (e.line(), e.column());
        if line == 0 {
            return 1; // Error at the start of the content
        }
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
}
impl Drop for BaseResponse {
    fn drop(&mut self) {
        self.inner_close()
    }
}

impl Response {
    pub fn new_py(py: Python, inner: BaseResponse) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self))
    }
}

#[pymethods]
impl BlockingResponse {
    fn bytes(slf: PyRefMut<Self>) -> PyResult<PyBytes> {
        Self::runtime(slf.as_ref())?.blocking_spawn(slf.into_super().bytes())
    }

    fn json(slf: PyRefMut<Self>) -> PyResult<JsonValue> {
        Self::runtime(slf.as_ref())?.blocking_spawn(slf.into_super().json())
    }

    fn text(slf: PyRefMut<Self>) -> PyResult<String> {
        Self::runtime(slf.as_ref())?.blocking_spawn(slf.into_super().text())
    }

    #[pyo3(signature = (amount=DEFAULT_READ_BUFFER_LIMIT))]
    fn read(slf: PyRefMut<Self>, amount: usize) -> PyResult<PyBytes> {
        Self::runtime(slf.as_ref())?.blocking_spawn(slf.into_super().read(amount))
    }

    fn next_chunk(slf: PyRefMut<Self>) -> PyResult<Option<PyBytes>> {
        Self::runtime(slf.as_ref())?.blocking_spawn(slf.into_super().next_chunk())
    }
}
impl BlockingResponse {
    pub fn new_py(py: Python, inner: BaseResponse) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self))
    }

    fn runtime(slf: &BaseResponse) -> PyResult<Handle> {
        match slf.runtime.clone() {
            Some(r) => Ok(r),
            None => Ok(Handle::global_handle()?.clone()),
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
