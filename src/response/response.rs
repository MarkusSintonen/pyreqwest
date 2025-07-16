use crate::exceptions::StatusError;
use crate::exceptions::utils::map_read_error;
use crate::http::Url;
use crate::http::{Extensions, HeaderMap, Version};
use bytes::Bytes;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3_bytes::PyBytes;
use pythonize::pythonize;
use serde_json::json;
use std::collections::VecDeque;
use tokio::sync::OwnedSemaphorePermit;

#[pyclass]
pub struct Response {
    #[pyo3(get, set)]
    status: u16,
    headers: Option<Py<PyAny>>,
    version: Option<Py<PyAny>>,
    extensions: Option<Py<PyAny>>,

    inner: Option<reqwest::Response>,
    request_semaphore_permit: Option<OwnedSemaphorePermit>,
    init_chunks: VecDeque<Bytes>,
    body_consuming_started: bool,
    read_body: Option<Bytes>,
}

#[pymethods]
impl Response {
    async fn __aenter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    async fn __aexit__(&mut self, _exc_type: Py<PyAny>, _exc_val: Py<PyAny>, _traceback: Py<PyAny>) {
        self.inner_close(); // Not actually async at the moment, but we have it async for the future
    }

    pub fn error_for_status(&mut self) -> PyResult<()> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Response was already consumed"))?;
        let inner = inner
            .error_for_status()
            .map_err(|e| StatusError::new_err(&e.to_string(), Some(json!({"status": e.status().unwrap().as_u16()}))))?;
        self.inner = Some(inner);
        Ok(())
    }

    #[getter]
    fn get_headers(&mut self, py: Python) -> PyResult<&Py<PyAny>> {
        if self.headers.is_none() {
            let headers = pythonize(py, &HeaderMap(self.try_ref()?.headers().clone()))?.unbind();
            self.headers = Some(headers);
        };
        Ok(self.headers.as_ref().unwrap())
    }

    #[setter]
    fn set_headers(&mut self, py: Python, headers: HeaderMap) -> PyResult<()> {
        self.headers = Some(pythonize(py, &headers)?.unbind());
        Ok(())
    }

    #[getter]
    fn get_version(&mut self, py: Python) -> PyResult<&Py<PyAny>> {
        if self.version.is_none() {
            let version = pythonize(py, &Version::from(self.try_ref()?.version()))?.unbind();
            self.version = Some(version);
        };
        Ok(self.version.as_ref().unwrap())
    }

    #[setter]
    fn set_version(&mut self, py: Python, version: Version) -> PyResult<()> {
        self.version = Some(pythonize(py, &version)?.unbind());
        Ok(())
    }

    #[getter]
    fn get_extensions(&mut self, py: Python) -> PyResult<&Py<PyAny>> {
        if self.extensions.is_none() {
            let ext = pythonize(py, &Extensions::from(self.try_ref()?.extensions()))?.unbind();
            self.extensions = Some(ext);
        };
        Ok(self.extensions.as_ref().unwrap())
    }

    #[setter]
    fn set_extensions(&mut self, py: Python, extensions: Extensions) -> PyResult<()> {
        self.extensions = Some(pythonize(py, &extensions)?.unbind());
        Ok(())
    }

    async fn next_chunk(&mut self) -> PyResult<Option<PyBytes>> {
        Ok(self.next_chunk_inner().await?.map(PyBytes::from))
    }

    async fn bytes(&mut self) -> PyResult<PyBytes> {
        self.bytes_inner().await
    }

    pub fn content_length(&self) -> PyResult<Option<u64>> {
        Ok(self.try_ref()?.content_length())
    }

    pub fn url(&self) -> PyResult<Url> {
        Ok(self.try_ref()?.url().clone().into())
    }

    pub fn remote_addr_ip_port(&self) -> PyResult<Option<(String, u16)>> {
        Ok(self.try_ref()?.remote_addr().map(|v| (v.ip().to_string(), v.port())))
    }

    async fn close(&mut self) {
        self.inner_close(); // Not actually async at the moment, but we have it async for the future
    }
}
impl Response {
    pub async fn initialize(
        mut response: reqwest::Response,
        mut request_semaphore_permit: Option<OwnedSemaphorePermit>,
    ) -> PyResult<Response> {
        let status = response.status().as_u16();

        let init_byte_limit = 65536;
        let (init_chunks, has_more) = Self::read_limit(&mut response, init_byte_limit).await?;

        let request_semaphore_permit = if has_more {
            request_semaphore_permit
        } else {
            // Release the semaphore right away without waiting for user to do it (by consuming or closing).
            request_semaphore_permit.take().map(drop);
            None
        };

        let resp = Response {
            status,
            headers: None,
            version: None,
            extensions: None,
            inner: Some(response),
            request_semaphore_permit,
            init_chunks,
            body_consuming_started: false,
            read_body: None,
        };
        Ok(resp)
    }

    async fn next_chunk_inner(&mut self) -> PyResult<Option<Bytes>> {
        self.body_consuming_started = true;

        if let Some(chunk) = self.init_chunks.pop_front() {
            return Ok(Some(chunk));
        }

        if let Some(inner) = self.inner.as_mut() {
            match inner.chunk().await.map_err(map_read_error)? {
                Some(chunk) => Ok(Some(chunk)),
                None => {
                    self.request_semaphore_permit.take().map(drop);
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }

    async fn bytes_inner(&mut self) -> PyResult<PyBytes> {
        if let Some(read_body) = self.read_body.as_ref() {
            return Ok(PyBytes::new(read_body.clone()));
        }

        if self.body_consuming_started {
            return Err(PyRuntimeError::new_err("Response body already consumed"));
        }

        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = self.next_chunk_inner().await? {
            bytes.extend(chunk);
        }

        let bytes = Bytes::from(bytes);
        self.read_body = Some(bytes.clone());
        Ok(PyBytes::new(bytes))
    }

    async fn read_limit(response: &mut reqwest::Response, byte_limit: usize) -> PyResult<(VecDeque<Bytes>, bool)> {
        let mut init_chunks: VecDeque<Bytes> = VecDeque::new();
        let mut has_more = true;
        let mut tot_bytes = 0;
        while has_more && (tot_bytes < byte_limit) {
            if let Some(chunk) = response.chunk().await.map_err(map_read_error)? {
                tot_bytes += chunk.len();
                init_chunks.push_back(chunk);
            } else {
                has_more = false;
            }
        }
        Ok((init_chunks, has_more))
    }

    fn inner_close(&mut self) {
        self.request_semaphore_permit.take().map(drop);
        self.inner.take().map(drop);
    }

    fn try_ref(&self) -> PyResult<&reqwest::Response> {
        self.inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Response was already consumed"))
    }
}
impl Drop for Response {
    fn drop(&mut self) {
        self.inner_close()
    }
}
