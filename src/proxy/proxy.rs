use std::panic::panic_any;
use crate::http::HeaderMap;
use crate::http::{Url, UrlType};
use http::HeaderValue;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use reqwest::NoProxy;

#[pyclass]
pub struct Proxy {
    inner: Option<reqwest::Proxy>,
}

#[pymethods]
impl Proxy {
    #[staticmethod]
    fn http(url: UrlType) -> PyResult<Self> {
        let proxy = reqwest::Proxy::http(url.0).map_err(|e| PyValueError::new_err(format!("Invalid proxy: {}", e)))?;
        Ok(Proxy { inner: Some(proxy) })
    }

    #[staticmethod]
    fn https(url: UrlType) -> PyResult<Self> {
        let proxy = reqwest::Proxy::https(url.0).map_err(|e| PyValueError::new_err(format!("Invalid proxy: {}", e)))?;
        Ok(Proxy { inner: Some(proxy) })
    }

    #[staticmethod]
    fn all(url: UrlType) -> PyResult<Self> {
        let proxy = reqwest::Proxy::all(url.0).map_err(|e| PyValueError::new_err(format!("Invalid proxy: {}", e)))?;
        Ok(Proxy { inner: Some(proxy) })
    }

    #[staticmethod]
    fn custom(fun: Py<PyAny>) -> PyResult<Self> {
        let proxy = reqwest::Proxy::custom(move |url| {
            match Self::handle_custom_proxy(&fun, url) {
                Ok(res) => res,
                Err(err) => panic_any(err) // No better way to handle this in reqwest custom proxy
            }
        });
        Ok(Proxy { inner: Some(proxy) })
    }

    fn basic_auth<'py>(slf: PyRefMut<'py, Self>, username: &str, password: &str) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| Ok(builder.basic_auth(username, password)))
    }

    fn custom_http_auth<'py>(slf: PyRefMut<'py, Self>, header_value: &str) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| {
            let val = HeaderValue::from_str(header_value)
                .map_err(|e| PyValueError::new_err(format!("Invalid header value: {}", e)))?;
            Ok(builder.custom_http_auth(val))
        })
    }

    fn headers<'py>(slf: PyRefMut<'py, Self>, headers: HeaderMap) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| Ok(builder.headers(headers.0)))
    }

    fn no_proxy<'py>(slf: PyRefMut<'py, Self>, no_proxy_list: Option<&str>) -> PyResult<PyRefMut<'py, Self>> {
        Self::apply(slf, |builder| Ok(builder.no_proxy(no_proxy_list.map(NoProxy::from_string).flatten())))
    }
}

impl Proxy {
    pub fn build(&mut self) -> PyResult<reqwest::Proxy> {
        self.inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Proxy was already built"))
    }

    fn handle_custom_proxy(fun: &Py<PyAny>, url: &reqwest::Url) -> PyResult<Option<reqwest::Url>> {
        Python::with_gil(|py| {
            Ok(fun.call1(py, (Url(url.clone()),))?.extract::<Option<UrlType>>(py)?.map(|v| v.0))
        })
    }

    fn apply<F>(mut slf: PyRefMut<Self>, fun: F) -> PyResult<PyRefMut<Self>>
    where
        F: FnOnce(reqwest::Proxy) -> PyResult<reqwest::Proxy>,
    {
        let builder = slf
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Proxy was already built"))?;
        slf.inner = Some(fun(builder)?);
        Ok(slf)
    }
}
