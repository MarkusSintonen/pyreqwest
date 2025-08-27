use crate::exceptions::utils::map_read_error;
use crate::exceptions::{JSONDecodeError, RequestError, StatusError};
use crate::http::{Extensions, HeaderMap, HeaderValue, Mime, Version};
use crate::http::{JsonValue, StatusCode};
use bytes::Bytes;
use encoding_rs::{Encoding, UTF_8};
use http_body_util::BodyExt;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
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

    headers: RespHeaders,
    extensions: RespExtensions,

    request_semaphore_permit: Option<OwnedSemaphorePermit>,
    body_stream: Option<reqwest::Body>,
    init_chunks: VecDeque<Bytes>,
    body_consuming_started: bool,
    read_body: Option<Bytes>,
}

#[pymethods]
impl Response {
    #[getter]
    fn get_headers(&mut self, py: Python) -> PyResult<&Py<HeaderMap>> {
        if let RespHeaders::Headers(headers) = &mut self.headers {
            let py_headers = Py::new(py, HeaderMap::from(headers.try_take_inner()?))?;
            self.headers = RespHeaders::PyHeaders(py_headers);
        }
        match &self.headers {
            RespHeaders::PyHeaders(py_headers) => Ok(py_headers),
            RespHeaders::Headers(_) => Err(PyRuntimeError::new_err("Expected PyHeaders")),
        }
    }

    #[setter]
    fn set_headers(&mut self, value: Py<HeaderMap>) -> PyResult<()> {
        self.headers = RespHeaders::PyHeaders(value);
        Ok(())
    }

    #[getter]
    fn get_extensions(&mut self, py: Python) -> PyResult<&Py<PyDict>> {
        if let RespExtensions::Extensions(ext) = &mut self.extensions {
            let py_ext = ext
                .remove::<Extensions>()
                .unwrap_or_else(|| Extensions(PyDict::new(py).unbind()))
                .0;
            self.extensions = RespExtensions::PyExtensions(py_ext);
        }
        match &self.extensions {
            RespExtensions::PyExtensions(py_ext) => Ok(py_ext),
            RespExtensions::Extensions(_) => Err(PyRuntimeError::new_err("Expected PyExtensions")),
        }
    }

    #[setter]
    fn set_extensions(&mut self, extensions: Extensions) {
        self.extensions = RespExtensions::PyExtensions(extensions.0);
    }

    async fn next_chunk(&mut self) -> PyResult<Option<PyBytes>> {
        Ok(self.next_chunk_inner().await?.map(PyBytes::from))
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
            .map(|mime| mime.get_param("charset").map(String::from))
            .flatten()
            .map(|charset| Encoding::for_label(charset.as_bytes()))
            .flatten()
            .unwrap_or(UTF_8);
        let (text, _, _) = encoding.decode(&bytes);
        Ok(text.into_owned())
    }

    fn content_type_mime(&self) -> PyResult<Option<Mime>> {
        let Some(content_type) = Python::with_gil(|py| self.get_header(py, "content-type"))? else {
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
}
impl Response {
    pub async fn initialize(
        mut response: reqwest::Response,
        mut request_semaphore_permit: Option<OwnedSemaphorePermit>,
        consume_body: BodyConsumeConfig,
    ) -> PyResult<Response> {
        let (head, init_chunks, body_stream, permit) = match consume_body {
            BodyConsumeConfig::Fully => {
                let (init_chunks, has_more) = Self::read_limit(&mut response, None).await?;
                assert_eq!(has_more, false, "Should have fully consumed the response");

                // Release the semaphore right away without waiting for user to do it (by consuming or closing).
                request_semaphore_permit.take().map(drop);

                let (head, body) = Self::response_parts(response);
                drop(body); // Was already read
                (head, init_chunks, None, None)
            }
            BodyConsumeConfig::Partially(amount) => {
                let (init_chunks, has_more) = Self::read_limit(&mut response, Some(amount)).await?;

                let (head, body) = Self::response_parts(response);
                let (body, permit) = if has_more {
                    (Some(body), request_semaphore_permit)
                } else {
                    // Release the semaphore right away without waiting for user to do it (by consuming or closing).
                    request_semaphore_permit.take().map(drop);
                    drop(body); // Was already read
                    (None, None)
                };
                (head, init_chunks, body, permit)
            }
        };

        let resp = Response {
            status: StatusCode(head.status),
            version: Version(head.version),
            headers: RespHeaders::Headers(HeaderMap::from(head.headers)),
            extensions: RespExtensions::Extensions(head.extensions),
            body_stream,
            request_semaphore_permit: permit,
            init_chunks,
            body_consuming_started: false,
            read_body: None,
        };
        Ok(resp)
    }

    fn get_header(&self, py: Python, name: &str) -> PyResult<Option<HeaderValue>> {
        match self.headers {
            RespHeaders::Headers(ref headers) => headers.get_one(name),
            RespHeaders::PyHeaders(ref py_headers) => py_headers.try_borrow(py)?.get_one(name),
        }
    }

    async fn next_chunk_inner(&mut self) -> PyResult<Option<Bytes>> {
        self.body_consuming_started = true;

        if let Some(chunk) = self.init_chunks.pop_front() {
            return Ok(Some(chunk));
        }

        if let Some(body_stream) = self.body_stream.as_mut() {
            loop {
                if let Some(frame) = body_stream.frame().await {
                    if let Ok(chunk) = frame.map_err(map_read_error)?.into_data() {
                        return Ok(Some(chunk));
                    } else {
                        // Skip non-DATA frame
                    }
                } else {
                    self.request_semaphore_permit.take().map(drop);
                    self.body_stream.take().map(drop);
                    return Ok(None); // All was consumed
                }
            }
        } else {
            Ok(None) // Nothing to consume
        }
    }

    async fn bytes_inner(&mut self) -> PyResult<Bytes> {
        if let Some(read_body) = self.read_body.as_ref() {
            return Ok(read_body.clone());
        }

        if self.body_consuming_started {
            return Err(PyRuntimeError::new_err("Response body already consumed"));
        }

        let mut bytes: Vec<u8> = vec![];
        while let Some(chunk) = self.next_chunk_inner().await? {
            bytes.extend(chunk);
        }

        let bytes = Bytes::from(bytes);
        self.read_body = Some(bytes.clone());
        Ok(bytes)
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
        self.request_semaphore_permit.take().map(drop);
        self.body_stream.take().map(drop);
    }

    async fn json_error(&mut self, e: &serde_json::error::Error) -> PyResult<PyErr> {
        let text = self.text().await?;
        let details = json!({"pos": Self::json_error_pos(&text, &e), "doc": text, "causes": serde_json::Value::Null});
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
    Partially(usize),
}

enum RespHeaders {
    Headers(HeaderMap),
    PyHeaders(Py<HeaderMap>), // In Python heap
}

enum RespExtensions {
    Extensions(http::Extensions),
    PyExtensions(Py<PyDict>), // In Python heap
}
