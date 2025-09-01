use crate::exceptions::utils::map_read_error;
use crate::exceptions::{JSONDecodeError, RequestError, StatusError};
use crate::http::{Extensions, HeaderMap, HeaderValue, Mime, Version};
use crate::http::{JsonValue, StatusCode};
use crate::response::bytes_channel::{Receiver, bytes_channel};
use bytes::{Bytes, BytesMut};
use encoding_rs::{Encoding, UTF_8};
use http_body_util::BodyExt;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::{PyTraverseError, PyVisit};
use pyo3_bytes::PyBytes;
use serde_json::json;
use std::collections::VecDeque;
use tokio::sync::OwnedSemaphorePermit;

#[pyclass(subclass)]
pub struct Response {
    #[pyo3(get, set)]
    status: StatusCode,
    #[pyo3(get, set)]
    version: Version,

    headers: Option<RespHeaders>,
    extensions: Option<RespExtensions>,

    chunks: VecDeque<Bytes>,
    body_consuming_started: bool,
    read_body: Option<Bytes>,
    body_rx: Option<Receiver>,
}

#[pymethods]
impl Response {
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

    async fn bytes(&mut self) -> PyResult<PyBytes> {
        Ok(PyBytes::new(self.bytes_inner().await?))
    }

    async fn json(&mut self) -> PyResult<JsonValue> {
        let bytes = self.bytes_inner().await?;
        match serde_json::from_slice(&bytes) {
            Ok(v) => Ok(v),
            Err(e) => Err(self.json_error(&e).await?),
        }
    }

    async fn text(&mut self) -> PyResult<String> {
        let bytes = self.bytes_inner().await?;
        let encoding = self
            .content_type_mime()?
            .and_then(|mime| mime.get_param("charset").map(String::from))
            .and_then(|charset| Encoding::for_label(charset.as_bytes()))
            .unwrap_or(UTF_8);
        let (text, _, _) = encoding.decode(&bytes);
        Ok(text.into_owned())
    }

    #[pyo3(signature = (amount=65536))]
    async fn read(&mut self, amount: usize) -> PyResult<PyBytes> {
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
    }

    async fn next_chunk(&mut self) -> PyResult<Option<PyBytes>> {
        Ok(self.next_chunk_inner().await?.map(PyBytes::from))
    }

    fn content_type_mime(&self) -> PyResult<Option<Mime>> {
        let Some(content_type) = self.get_header("content-type")? else {
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
impl Response {
    pub async fn initialize(
        mut response: reqwest::Response,
        mut request_semaphore_permit: Option<OwnedSemaphorePermit>,
        consume_body: BodyConsumeConfig,
    ) -> PyResult<Response> {
        let (head, init_chunks, rx) = match consume_body {
            BodyConsumeConfig::Fully => {
                let (init_chunks, has_more) = Self::read_limit(&mut response, None).await?;
                assert!(!has_more, "Should have fully consumed the response");

                // Release the semaphore right away without waiting for user to do it (by consuming or closing).
                _ = request_semaphore_permit.take();

                let (head, _body) = Self::response_parts(response); // Body was fully read, drops it

                (head, init_chunks, None)
            }
            BodyConsumeConfig::Partially(conf) => {
                let (init_chunks, has_more) = Self::read_limit(&mut response, Some(conf.initial_read_size)).await?;

                let (head, mut body) = Self::response_parts(response);

                let (mut tx, rx) = bytes_channel(conf.read_buffer_size);

                if has_more {
                    tokio::runtime::Handle::current().spawn(async move {
                        loop {
                            match body.frame().await.transpose().map_err(map_read_error) {
                                Err(e) => {
                                    let _ = tx.send(Err(e)).await;
                                    break; // Stop on error
                                }
                                Ok(None) => {
                                    tx.finalize().await;
                                    break; // All was consumed
                                }
                                Ok(Some(frame)) => {
                                    if let Ok(chunk) = frame.into_data() {
                                        if !tx.send(Ok(chunk)).await {
                                            break; // Receiver was dropped
                                        }
                                    }
                                }
                            }
                        }
                        _ = request_semaphore_permit.take();
                        drop(body);
                    });
                } else {
                    _ = request_semaphore_permit.take();
                    drop(body); // Was already read
                };

                (head, init_chunks, Some(rx))
            }
        };

        let resp = Response {
            status: StatusCode(head.status),
            version: Version(head.version),
            headers: Some(RespHeaders::Headers(HeaderMap::from(head.headers))),
            extensions: Some(RespExtensions::Extensions(head.extensions)),
            chunks: init_chunks,
            body_consuming_started: false,
            read_body: None,
            body_rx: rx,
        };
        Ok(resp)
    }

    fn get_header(&self, name: &str) -> PyResult<Option<HeaderValue>> {
        match self.headers {
            Some(RespHeaders::Headers(ref headers)) => headers.get_one(name),
            Some(RespHeaders::PyHeaders(ref py_headers)) => {
                Python::with_gil(|py| py_headers.try_borrow(py)?.get_one(name))
            }
            None => Err(PyRuntimeError::new_err("Expected headers")),
        }
    }

    async fn next_chunk_inner(&mut self) -> PyResult<Option<Bytes>> {
        self.body_consuming_started = true;

        if let Some(chunk) = self.chunks.pop_front() {
            return Ok(Some(chunk));
        }

        let Some(body_rx) = self.body_rx.as_mut() else {
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
        if let Some(read_body) = self.read_body.as_ref() {
            return Ok(read_body.clone()); // Zero-copy clone
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
        self.read_body = Some(bytes.clone()); // Zero-copy clone
        Ok(bytes)
    }

    fn content_length(&self) -> PyResult<Option<usize>> {
        let Some(content_type) = self.get_header("content-length")? else {
            return Ok(None);
        };
        content_type
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

    pub fn inner_close(&mut self) {
        if let Some(rx) = self.body_rx.as_mut() { rx.close() } // Close the receiver to stop the reader background task
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
impl Drop for Response {
    fn drop(&mut self) {
        self.inner_close()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BodyConsumeConfig {
    Fully,
    Partially(PartialReadConfig),
}

#[derive(Debug, Clone, Copy)]
pub struct PartialReadConfig {
    pub initial_read_size: usize,
    pub read_buffer_size: usize,
}

enum RespHeaders {
    Headers(HeaderMap),
    PyHeaders(Py<HeaderMap>), // In Python heap
}

enum RespExtensions {
    Extensions(http::Extensions),
    PyExtensions(Py<PyDict>), // In Python heap
}
