use crate::http_types::{HeaderMapExt, UrlExt};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use reqwest::Proxy;

#[pyclass(frozen)]
pub struct ProxyConfig {
    #[pyo3(get)]
    url: UrlExt,
    #[pyo3(get)]
    basic_auth: Option<(String, String)>,
    #[pyo3(get)]
    headers: Option<HeaderMapExt>,
}

#[pymethods]
impl ProxyConfig {
    #[new]
    fn py_new(url: UrlExt, basic_auth: Option<(String, String)>, headers: Option<HeaderMapExt>) -> PyResult<Self> {
        Ok(ProxyConfig {
            url,
            basic_auth,
            headers,
        })
    }
}

impl ProxyConfig {
    pub fn build_reqwest_proxy(&self) -> PyResult<Proxy> {
        let url: reqwest::Url = self.url.clone().try_into()?;

        let mut proxy = Proxy::all(url).map_err(|e| PyValueError::new_err(format!("Invalid Proxy URL: {}", e)))?;

        if let Some((username, password)) = &self.basic_auth {
            proxy = proxy.basic_auth(username, password);
        }
        if let Some(headers) = &self.headers {
            // Check there is only Proxy-Authorization header
            // https://github.com/seanmonstar/reqwest/issues/2552
            if headers.0.len() > 1 {
                return Err(PyValueError::new_err("Only Proxy-Authorization header is allowed, for now."));
            }
            if let Some((name, value)) = headers.0.iter().next() {
                if name.as_str().to_lowercase() != "proxy-authorization" {
                    return Err(PyValueError::new_err("Only Proxy-Authorization header is allowed, for now."));
                }
                proxy = proxy.custom_http_auth(value.clone());
            }
        }

        Ok(proxy)
    }
}
