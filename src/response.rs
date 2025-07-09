use crate::http_types::{Extensions, HeaderMapExt, StatusCodeExt, VersionExt};
use crate::utils::map_read_error;
use bytes::Bytes;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3_bytes::PyBytes;
use pythonize::pythonize;
use std::collections::VecDeque;
use tokio::sync::OwnedSemaphorePermit;

#[pyclass]
pub struct Response {
    #[pyo3(get)]
    status_code: Py<PyAny>,
    #[pyo3(get)]
    headers: Py<PyAny>,
    #[pyo3(get)]
    http_version: Py<PyAny>,
    #[pyo3(get)]
    extensions: Py<PyAny>,

    inner: Option<reqwest::Response>,
    request_semaphore_permit: Option<OwnedSemaphorePermit>,
    init_chunks: VecDeque<PyBytes>,
    body_consuming_started: bool,
    read_body: Option<Py<PyBytes>>,
}

#[pymethods]
impl Response {
    async fn __aenter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    async fn __aexit__(&mut self, _exc_type: Py<PyAny>, _exc_val: Py<PyAny>, _traceback: Py<PyAny>) {
        self.inner_close(); // Not actually async at the moment, but we have it async for the future
    }

    async fn next_chunk(&mut self) -> PyResult<Option<PyBytes>> {
        self.body_consuming_started = true;

        if let Some(chunk) = self.init_chunks.pop_front() {
            return Ok(Some(chunk));
        }

        if let Some(inner) = self.inner.as_mut() {
            match inner.chunk().await.map_err(map_read_error)? {
                Some(chunk) => Ok(Some(PyBytes::new(chunk))),
                None => {
                    self.inner_close(); // No more chunks available, so close the response
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }

    async fn read(&mut self) -> PyResult<Py<PyBytes>> {
        if let Some(read_body) = self.read_body.as_ref() {
            return Ok(Python::with_gil(|py| read_body.clone_ref(py)));
        }

        if self.body_consuming_started {
            return Err(PyRuntimeError::new_err("Response body already consumed"));
        }

        let mut bytes: Vec<u8> = Vec::new();
        for chunk in self.init_chunks.drain(..) {
            bytes.extend(chunk.into_inner());
        }
        while let Some(chunk) = self.next_chunk().await? {
            bytes.extend(chunk.into_inner());
        }

        let py_bytes = PyBytes::new(Bytes::from(bytes));
        Python::with_gil(|py| {
            let py_bytes = Py::new(py, py_bytes)?;
            self.read_body = Some(py_bytes.clone_ref(py));
            Ok(py_bytes)
        })
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
        let status_code = StatusCodeExt::from(response.status());
        let headers = HeaderMapExt(response.headers().clone());
        let http_version = VersionExt::from(response.version());
        let extensions = Extensions::from(response.extensions());

        let init_byte_limit = 65536;
        let (init_chunks, has_more) = Self::read_limit(&mut response, init_byte_limit).await?;

        let (resp, request_permit) = if has_more {
            (Some(response), request_semaphore_permit)
        } else {
            // Release the semaphore right away and drop the response
            // without waiting for user to do it (by consuming or closing).
            request_semaphore_permit.take().map(drop);
            drop(response);
            (None, None)
        };

        Python::with_gil(|py| {
            let resp = Response {
                status_code: pythonize(py, &status_code)?.unbind(),
                headers: pythonize(py, &headers)?.unbind(),
                http_version: pythonize(py, &http_version)?.unbind(),
                extensions: pythonize(py, &extensions)?.unbind(),
                inner: resp,
                request_semaphore_permit: request_permit,
                init_chunks,
                body_consuming_started: false,
                read_body: None,
            };
            Ok(resp)
        })
    }

    async fn read_limit(response: &mut reqwest::Response, byte_limit: usize) -> PyResult<(VecDeque<PyBytes>, bool)> {
        let mut init_chunks: VecDeque<PyBytes> = VecDeque::new();
        let mut has_more = true;
        let mut tot_bytes = 0;
        while has_more && (tot_bytes < byte_limit) {
            if let Some(chunk) = response.chunk().await.map_err(map_read_error)? {
                tot_bytes += chunk.len();
                init_chunks.push_back(PyBytes::new(chunk));
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
}
impl Drop for Response {
    fn drop(&mut self) {
        self.inner_close()
    }
}
