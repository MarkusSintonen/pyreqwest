use crate::exceptions::utils::map_send_error;
use crate::http_types::{Extensions, HeaderMapExt, MethodExt, UrlExt};
use crate::request::RequestBody;
use http::{HeaderName, HeaderValue};
use pyo3::PyResult;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use std::str::FromStr;

pub struct RequestWrapper {
    inner: Option<reqwest::Request>,
    body: Option<RequestBody>,
    extensions: Option<Extensions>,
}

impl RequestWrapper {
    pub fn new(request: reqwest::Request, body: Option<RequestBody>, extensions: Option<Extensions>) -> Self {
        RequestWrapper {
            inner: Some(request),
            extensions,
            body,
        }
    }

    pub fn get_method(&self) -> PyResult<MethodExt> {
        Ok(self.inner_ref()?.method().clone().into())
    }

    pub fn set_method(&mut self, value: MethodExt) -> PyResult<()> {
        *self.inner_mut()?.method_mut() = value.0;
        Ok(())
    }

    pub fn get_url(&self) -> PyResult<UrlExt> {
        self.inner_ref()?.url().clone().try_into()
    }

    pub fn set_url(&mut self, value: UrlExt) -> PyResult<()> {
        *self.inner_mut()?.url_mut() = value.try_into()?;
        Ok(())
    }

    pub fn copy_headers(&self) -> PyResult<HeaderMapExt> {
        Ok(self.inner_ref()?.headers().clone().into())
    }

    pub fn set_headers(&mut self, value: HeaderMapExt) -> PyResult<()> {
        *self.inner_mut()?.headers_mut() = value.0;
        Ok(())
    }

    pub fn get_header(&self, key: &str) -> PyResult<Option<String>> {
        self.inner_ref()?
            .headers()
            .get(key)
            .map(|v| {
                v.to_str()
                    .map(|s| s.to_string())
                    .map_err(|e| PyRuntimeError::new_err(format!("Invalid header value: {}", e)))
            })
            .transpose()
    }

    pub fn set_header(&mut self, key: &str, value: &str) -> PyResult<Option<String>> {
        let key =
            HeaderName::from_str(key).map_err(|e| PyValueError::new_err(format!("Invalid header name: {}", e)))?;
        let value =
            HeaderValue::from_str(value).map_err(|e| PyValueError::new_err(format!("Invalid header value: {}", e)))?;
        self.inner_mut()?
            .headers_mut()
            .insert(key, value)
            .map(|v| {
                v.to_str()
                    .map(|s| s.to_string())
                    .map_err(|e| PyRuntimeError::new_err(format!("Invalid header value: {}", e)))
            })
            .transpose()
    }

    pub fn copy_body(&self) -> PyResult<Option<RequestBody>> {
        self.body.as_ref().map(|b| b.try_clone()).transpose()
    }

    pub fn set_body(&mut self, value: Bound<PyAny>) -> PyResult<()> {
        if value.is_none() {
            self.body = None;
        } else {
            self.body = Some(value.downcast::<RequestBody>()?.try_borrow()?.try_clone()?);
        }
        Ok(())
    }

    pub fn copy_extensions(&self) -> Option<Extensions> {
        self.extensions.clone()
    }

    pub fn set_extensions(&mut self, value: Option<Extensions>) {
        self.extensions = value;
    }

    fn inner_ref(&self) -> PyResult<&reqwest::Request> {
        self.inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already consumed"))
    }

    fn inner_mut(&mut self) -> PyResult<&mut reqwest::Request> {
        self.inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already consumed"))
    }

    pub fn into_parts(&mut self) -> PyResult<(reqwest::Request, Option<RequestBody>, Option<Extensions>)> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already consumed"))?;
        let body = self.body.take();
        let extensions = self.extensions.take();
        Ok((inner, body, extensions))
    }

    pub async fn execute(&mut self, client: &reqwest::Client) -> PyResult<reqwest::Response> {
        let (inner, body, ext) = self.into_parts()?;
        Self::inner_execute(inner, body, ext, client).await
    }

    pub async fn inner_execute(
        mut inner: reqwest::Request,
        body: Option<RequestBody>,
        ext: Option<Extensions>,
        client: &reqwest::Client,
    ) -> PyResult<reqwest::Response> {
        *inner.body_mut() = body.map(|b| b.try_into()).transpose()?;
        let mut resp = client.execute(inner).await.map_err(map_send_error)?;
        ext.map(|ext| Self::move_extensions(ext, resp.extensions_mut()));
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

    fn move_extensions(from: Extensions, to: &mut http::Extensions) -> &mut Extensions {
        let to = to.get_or_insert_default::<Extensions>();
        for (k, v) in from.0.into_iter() {
            if !to.0.contains_key(&k) {
                to.0.insert(k, v);
            }
        }
        to
    }
}
