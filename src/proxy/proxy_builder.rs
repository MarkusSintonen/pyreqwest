use crate::http::HeaderMap;
use crate::http::{Url, UrlType};
use crate::internal::types::HeaderValue;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use reqwest::NoProxy;
use std::panic::panic_any;

#[pyclass]
pub struct ProxyBuilder {
    inner: Option<reqwest::Proxy>,
}

#[pymethods]
impl ProxyBuilder {
    #[staticmethod]
    fn http(py: Python, url: UrlType) -> PyResult<Self> {
        py.detach(|| {
            let proxy =
                reqwest::Proxy::http(url.0).map_err(|e| PyValueError::new_err(format!("Invalid proxy: {}", e)))?;
            Ok(ProxyBuilder { inner: Some(proxy) })
        })
    }

    #[staticmethod]
    fn https(py: Python, url: UrlType) -> PyResult<Self> {
        py.detach(|| {
            let proxy =
                reqwest::Proxy::https(url.0).map_err(|e| PyValueError::new_err(format!("Invalid proxy: {}", e)))?;
            Ok(ProxyBuilder { inner: Some(proxy) })
        })
    }

    #[staticmethod]
    fn all(py: Python, url: UrlType) -> PyResult<Self> {
        py.detach(|| {
            let proxy =
                reqwest::Proxy::all(url.0).map_err(|e| PyValueError::new_err(format!("Invalid proxy: {}", e)))?;
            Ok(ProxyBuilder { inner: Some(proxy) })
        })
    }

    #[staticmethod]
    fn custom(fun: Py<PyAny>) -> PyResult<Self> {
        let proxy = reqwest::Proxy::custom(move |url| {
            match Self::handle_custom_proxy(&fun, url) {
                Ok(res) => res,
                #[allow(clippy::panic)]
                Err(err) => panic_any(err), // No better way to handle this in reqwest custom proxy
            }
        });
        Ok(ProxyBuilder { inner: Some(proxy) })
    }

    fn basic_auth<'py>(slf: PyRefMut<'py, Self>, username: &str, password: &str) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| Ok(builder.basic_auth(username, password)))
    }

    fn custom_http_auth(slf: PyRefMut<'_, Self>, header_value: HeaderValue) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |builder| Ok(builder.custom_http_auth(header_value.0)))
    }

    fn headers(slf: PyRefMut<'_, Self>, headers: HeaderMap) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |builder| Ok(builder.headers(headers.try_take_inner()?)))
    }

    fn no_proxy<'py>(slf: PyRefMut<'py, Self>, no_proxy_list: Option<&str>) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| Ok(builder.no_proxy(no_proxy_list.and_then(NoProxy::from_string))))
    }
}

impl ProxyBuilder {
    pub fn build(&mut self) -> PyResult<reqwest::Proxy> {
        self.inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Proxy was already built"))
    }

    fn handle_custom_proxy(fun: &Py<PyAny>, url: &reqwest::Url) -> PyResult<Option<reqwest::Url>> {
        Python::attach(|py| {
            Ok(fun
                .call1(py, (Url::from(url.clone()),))?
                .extract::<Option<UrlType>>(py)?
                .map(|v| v.0))
        })
    }

    fn apply<F>(mut slf: PyRefMut<Self>, fun: F) -> PyResult<PyRefMut<Self>>
    where
        F: FnOnce(reqwest::Proxy) -> PyResult<reqwest::Proxy>,
        F: Send,
    {
        let builder = slf
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Proxy was already built"))?;
        slf.inner = Some(slf.py().detach(|| fun(builder))?);
        Ok(slf)
    }
}
