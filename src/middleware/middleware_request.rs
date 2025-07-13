use crate::http::types::{Extensions, HeaderMap, Method};
use crate::http::url::{Url, UrlType};
use crate::request::{RequestBody, RequestWrapper};
use crate::response::Response;
use pyo3::{Bound, Py, PyAny, PyResult, Python, pyclass, pymethods};
use std::sync::Arc;
use tokio::sync::OwnedSemaphorePermit;

#[pyclass]
pub struct MiddlewareRequest {
    inner: RequestWrapper,
}

#[pymethods]
impl MiddlewareRequest {
    #[getter]
    fn get_method(&self) -> PyResult<Method> {
        self.inner.get_method()
    }

    #[setter]
    fn set_method(&mut self, value: Method) -> PyResult<()> {
        self.inner.set_method(value)
    }

    #[getter]
    fn get_url(&self) -> PyResult<Url> {
        self.inner.get_url()
    }

    #[setter]
    fn set_url(&mut self, value: UrlType) -> PyResult<()> {
        self.inner.set_url(value)
    }

    fn copy_headers(&self) -> PyResult<HeaderMap> {
        self.inner.copy_headers()
    }

    fn set_headers(&mut self, value: HeaderMap) -> PyResult<()> {
        self.inner.set_headers(value)
    }

    fn get_header(&self, key: &str) -> PyResult<Option<String>> {
        self.inner.get_header(key)
    }

    fn set_header(&mut self, key: &str, value: &str) -> PyResult<Option<String>> {
        self.inner.set_header(key, value)
    }

    fn copy_body(&self) -> PyResult<Option<RequestBody>> {
        self.inner.copy_body()
    }

    fn set_body<'py>(&mut self, value: Bound<'py, PyAny>) -> PyResult<()> {
        self.inner.set_body(value)
    }

    fn copy_extensions(&self) -> Option<Extensions> {
        self.inner.copy_extensions()
    }

    fn set_extensions(&mut self, value: Option<Extensions>) {
        self.inner.set_extensions(value)
    }

    fn copy(&mut self) -> PyResult<Self> {
        Ok(MiddlewareRequest {
            inner: self.inner.try_clone()?,
        })
    }

    fn __copy__(&mut self) -> PyResult<Self> {
        self.copy()
    }
}
impl MiddlewareRequest {
    pub fn new(inner: RequestWrapper) -> Self {
        MiddlewareRequest { inner }
    }

    pub async fn execute(
        slf: Py<Self>,
        client: Arc<reqwest::Client>,
        request_semaphore_permit: Option<OwnedSemaphorePermit>,
    ) -> PyResult<Response> {
        let (inner, body, ext) = Python::with_gil(|py| slf.try_borrow_mut(py)?.inner.into_parts())?;
        let resp = RequestWrapper::inner_execute(inner, body, ext, &client).await?;
        Response::initialize(resp, request_semaphore_permit).await
    }
}
