use crate::http_types::{Extensions, HeaderMapExt, MethodExt, UrlExt};
use crate::request_body::RequestBody;
use crate::utils::{map_send_error, move_extensions};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::{PyResult, pymethods};

#[pyclass]
pub struct RequestWrapper {
    inner: Option<reqwest::Request>,
    body: Option<RequestBody>,
    extensions: Option<Extensions>,
}

#[pymethods]
impl RequestWrapper {
    #[getter]
    fn get_method(&self) -> PyResult<MethodExt> {
        Ok(self.try_get_request()?.method().clone().into())
    }

    #[setter]
    fn set_method(&mut self, value: MethodExt) -> PyResult<()> {
        *self.try_mut_request()?.method_mut() = value.0;
        Ok(())
    }

    #[getter]
    fn get_url(&self) -> PyResult<UrlExt> {
        self.try_get_request()?.url().clone().try_into()
    }

    #[setter]
    fn set_url(&mut self, value: UrlExt) -> PyResult<()> {
        *self.try_mut_request()?.url_mut() = value.try_into()?;
        Ok(())
    }

    fn get_headers(&self) -> PyResult<HeaderMapExt> {
        Ok(self.try_get_request()?.headers().clone().into())
    }

    fn set_headers(&mut self, value: HeaderMapExt) -> PyResult<()> {
        *self.try_mut_request()?.headers_mut() = value.0;
        Ok(())
    }

    fn get_body(&self) -> PyResult<Option<RequestBody>> {
        self.body.as_ref().map(|b| b.try_clone()).transpose()
    }

    fn set_body<'py>(&mut self, value: Bound<'py, PyAny>) -> PyResult<()> {
        if value.is_none() {
            self.body = None;
        } else {
            self.body = Some(value.downcast::<RequestBody>()?.try_borrow()?.try_clone()?);
        }
        Ok(())
    }

    fn get_extensions(&self) -> Option<Extensions> {
        self.extensions.clone()
    }

    fn set_extensions(&mut self, value: Option<Extensions>) {
        self.extensions = value;
    }

    pub fn __copy__(&mut self) -> PyResult<Self> {
        self.try_clone()
    }
}
impl RequestWrapper {
    pub fn new(request: reqwest::Request, body: Option<RequestBody>, extensions: Option<Extensions>) -> Self {
        RequestWrapper {
            inner: Some(request),
            extensions,
            body,
        }
    }

    fn try_get_request(&self) -> PyResult<&reqwest::Request> {
        self.inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already consumed"))
    }

    fn try_mut_request(&mut self) -> PyResult<&mut reqwest::Request> {
        self.inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already consumed"))
    }

    pub async fn execute(&mut self, client: &reqwest::Client) -> PyResult<reqwest::Response> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already consumed"))?;

        Self::inner_execute(inner, self.body.take(), self.extensions.take(), client).await
    }

    pub async fn py_execute(slf: Py<Self>, client: &reqwest::Client) -> PyResult<reqwest::Response> {
        let (inner, body, ext) = Python::with_gil(|py| {
            let mut slf = slf.try_borrow_mut(py)?;
            let inner = slf
                .inner
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Request was already consumed"))?;
            Ok::<_, PyErr>((inner, slf.body.take(), slf.extensions.take()))
        })?;

        Self::inner_execute(inner, body, ext, client).await
    }

    async fn inner_execute(
        mut inner: reqwest::Request,
        body: Option<RequestBody>,
        ext: Option<Extensions>,
        client: &reqwest::Client,
    ) -> PyResult<reqwest::Response> {
        *inner.body_mut() = body.map(|b| b.try_into()).transpose()?;
        let mut resp = client.execute(inner).await.map_err(map_send_error)?;
        ext.map(|ext| move_extensions(ext, resp.extensions_mut()));
        Ok(resp)
    }

    pub fn try_clone(&mut self) -> PyResult<Self> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already sent"))?;
        let new_inner = inner
            .try_clone()
            .ok_or_else(|| PyRuntimeError::new_err("Failed to clone request"))?;
        self.inner = Some(inner);

        Ok(RequestWrapper {
            inner: Some(new_inner),
            body: self.body.as_ref().map(|b| b.try_clone()).transpose()?,
            extensions: self.extensions.clone(),
        })
    }
}
