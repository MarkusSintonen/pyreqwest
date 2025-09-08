use crate::allow_threads::AllowThreads;
use crate::client::Handle;
use crate::exceptions::utils::map_read_error;
use crate::exceptions::{JSONDecodeError, RequestError, StatusError};
use crate::http::{Extensions, HeaderMap, HeaderValue, Mime, Version};
use crate::http::{JsonValue, StatusCode};
use crate::response::body_read_channel::{Receiver, body_read_channel};
use bytes::{Bytes, BytesMut};
use encoding_rs::{Encoding, UTF_8};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::{PyTraverseError, PyVisit};
use pyo3_bytes::PyBytes;
use serde_json::json;
use std::collections::VecDeque;
use tokio::sync::OwnedSemaphorePermit;

#[pyclass(subclass)]
pub struct BaseResponse {
    #[pyo3(get, set)]
    status: StatusCode,
    #[pyo3(get, set)]
    version: Version,

    headers: Option<RespHeaders>,
    extensions: Option<RespExtensions>,

    chunks: VecDeque<Bytes>,
    body_consuming_started: bool,
    fully_consumed_body: Option<Bytes>,
    body_receiver: Option<Receiver>,
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
            let bytes = self.bytes_inner().await?;
            Ok(PyBytes::new(bytes))
        })
        .await
    }

    async fn json(&mut self) -> PyResult<JsonValue> {
        AllowThreads(async {
            let bytes = self.bytes_inner().await?;
            match serde_json::from_slice(&bytes) {
                Ok(v) => Ok(v),
                Err(e) => Err(self.json_error(&e).await?),
            }
        })
        .await
    }

    async fn text(&mut self) -> PyResult<String> {
        AllowThreads(async {
            let bytes = self.bytes_inner().await?;
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
        AllowThreads(async {
            let mut collected = BytesMut::with_capacity(amount);
            let mut remaining = amount;

            while remaining > 0 {
                if let Some(mut chunk) = self.next_chunk_inner().await? {
                    if chunk.len() > remaining {
                        let extra = chunk.split_off(remaining);
                        self.chunks.push_front(extra);
                    }
                    collected.extend_from_slice(&chunk);
                    remaining -= chunk.len();
                } else {
                    break; // No more data
                }
            }

            Ok(PyBytes::new(collected.freeze()))
        })
        .await
    }

    async fn next_chunk(&mut self) -> PyResult<Option<PyBytes>> {
        AllowThreads(async { Ok(self.next_chunk_inner().await?.map(PyBytes::from)) }).await
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
        mut response: reqwest::Response,
        mut request_semaphore_permit: Option<OwnedSemaphorePermit>,
        consume_body: BodyConsumeConfig,
        runtime: Option<Handle>,
    ) -> PyResult<Self> {
        let (init_chunks, has_more);
        let head: http::response::Parts;

        let body_receiver = match consume_body {
            BodyConsumeConfig::FullyConsumed => {
                (init_chunks, has_more) = Self::read_limit(&mut response, None).await?;
                assert!(!has_more, "Should have fully consumed the response");

                // Release the semaphore right away without waiting for user to do it (by consuming or closing).
                _ = request_semaphore_permit.take();

                (head, _) = Self::response_parts(response); // Body was fully read, drops it
                None
            }
            BodyConsumeConfig::Streamed(conf) => {
                (init_chunks, has_more) = Self::read_limit(&mut response, Some(conf.read_buffer_limit)).await?;

                let body;
                (head, body) = Self::response_parts(response);

                if has_more {
                    Some(body_read_channel(body, request_semaphore_permit, conf.read_buffer_limit, runtime.clone()))
                } else {
                    _ = request_semaphore_permit.take();
                    drop(body); // Was already read
                    None
                }
            }
        };

        let resp = BaseResponse {
            status: StatusCode(head.status),
            version: Version(head.version),
            headers: Some(RespHeaders::Headers(HeaderMap::from(head.headers))),
            extensions: Some(RespExtensions::Extensions(head.extensions)),
            chunks: init_chunks,
            body_consuming_started: false,
            fully_consumed_body: None,
            body_receiver,
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

    async fn next_chunk_inner(&mut self) -> PyResult<Option<Bytes>> {
        self.body_consuming_started = true;

        if let Some(chunk) = self.chunks.pop_front() {
            return Ok(Some(chunk));
        }

        let Some(body_rx) = self.body_receiver.as_mut() else {
            return Ok(None); // No body receiver, fully consumed
        };

        let Some(buffer) = body_rx.recv().await? else {
            return Ok(None); // No more data
        };
        if buffer.is_empty() {
            return Ok(None); // No more data
        }

        let mut buffer_iter = buffer.into_iter();
        let first_chunk = buffer_iter.next().unwrap();
        for rest_chunk in buffer_iter {
            self.chunks.push_back(rest_chunk);
        }
        Ok(Some(first_chunk))
    }

    async fn bytes_inner(&mut self) -> PyResult<Bytes> {
        if let Some(fully_consumed_body) = self.fully_consumed_body.as_ref() {
            return Ok(fully_consumed_body.clone()); // Zero-copy clone
        }

        if self.body_consuming_started {
            return Err(PyRuntimeError::new_err("Response body already consumed"));
        }

        let mut bytes = match self.content_length()? {
            Some(len) => BytesMut::with_capacity(len),
            None => BytesMut::new(),
        };

        while let Some(chunk) = self.next_chunk_inner().await? {
            bytes.extend_from_slice(&chunk);
        }

        let bytes = bytes.freeze();
        self.fully_consumed_body = Some(bytes.clone()); // Zero-copy clone
        Ok(bytes)
    }

    fn content_length(&self) -> PyResult<Option<usize>> {
        let Some(content_length) = self.get_header_inner("content-length")? else {
            return Ok(None);
        };
        content_length
            .0
            .to_str()
            .map_err(|e| RequestError::from_err("Invalid Content-Length header", &e))?
            .parse::<usize>()
            .map_err(|e| RequestError::from_err("Failed to parse Content-Length header", &e))
            .map(Some)
    }

    fn response_parts(response: reqwest::Response) -> (http::response::Parts, reqwest::Body) {
        let resp: http::Response<reqwest::Body> = response.into();
        resp.into_parts()
    }

    async fn read_limit(
        response: &mut reqwest::Response,
        byte_limit: Option<usize>,
    ) -> PyResult<(VecDeque<Bytes>, bool)> {
        if byte_limit == Some(0) {
            return Ok((VecDeque::new(), true));
        }

        let mut init_chunks: VecDeque<Bytes> = VecDeque::new();
        let mut has_more = true;
        let mut consumed_bytes = 0;
        while has_more {
            if let Some(chunk) = response.chunk().await.map_err(map_read_error)? {
                consumed_bytes += chunk.len();
                init_chunks.push_back(chunk);
                if let Some(byte_limit) = byte_limit {
                    if consumed_bytes >= byte_limit {
                        break;
                    }
                }
            } else {
                has_more = false;
            }
        }
        Ok((init_chunks, has_more))
    }

    pub fn inner_close(&self) {
        if let Some(rx) = self.body_receiver.as_ref() {
            rx.close() // Close the receiver to stop the reader background task
        }
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

#[derive(Debug, Clone, Copy)]
pub enum BodyConsumeConfig {
    FullyConsumed,
    Streamed(StreamedReadConfig),
}

pub const DEFAULT_READ_BUFFER_LIMIT: usize = 65536;

#[derive(Debug, Clone, Copy)]
pub struct StreamedReadConfig {
    pub read_buffer_limit: usize,
}

enum RespHeaders {
    Headers(HeaderMap),
    PyHeaders(Py<HeaderMap>), // In Python heap
}

enum RespExtensions {
    Extensions(http::Extensions),
    PyExtensions(Py<PyDict>), // In Python heap
}
