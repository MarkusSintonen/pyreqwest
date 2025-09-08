use crate::http::UrlType;
use crate::http::cookie::Cookie;
use crate::http::cookie::cookie::CookieType;
use bytes::Bytes;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::sync::RwLock;

#[pyclass(frozen)]
pub struct CookieStore(RwLock<cookie_store::CookieStore>);

#[pymethods]
impl CookieStore {
    #[new]
    fn new() -> Self {
        Self(RwLock::new(cookie_store::CookieStore::new()))
    }

    fn matches(&self, url: UrlType) -> Vec<Cookie> {
        self.0
            .read()
            .unwrap()
            .matches(&url.0)
            .into_iter()
            .map(Cookie::from)
            .collect()
    }

    fn contains(&self, domain: &str, path: &str, name: &str) -> bool {
        self.0.read().unwrap().contains(domain, path, name)
    }

    fn contains_any(&self, domain: &str, path: &str, name: &str) -> bool {
        self.0.read().unwrap().contains_any(domain, path, name)
    }

    fn get(&self, domain: &str, path: &str, name: &str) -> Option<Cookie> {
        self.0.read().unwrap().get(domain, path, name).map(Cookie::from)
    }

    fn get_any(&self, domain: &str, path: &str, name: &str) -> Option<Cookie> {
        self.0.read().unwrap().get_any(domain, path, name).map(Cookie::from)
    }

    fn remove(&self, domain: &str, path: &str, name: &str) -> Option<Cookie> {
        self.0.write().unwrap().remove(domain, path, name).map(Cookie::from)
    }

    fn insert(&self, cookie: CookieType, request_url: UrlType) -> PyResult<()> {
        self.0
            .write()
            .unwrap()
            .insert_raw(&cookie.0, &request_url.0)
            .map_err(|e| PyValueError::new_err(e.to_string()))
            .map(|_| ())
    }

    fn clear(&self) {
        self.0.write().unwrap().clear();
    }

    fn get_all_unexpired(&self) -> Vec<Cookie> {
        self.0.read().unwrap().iter_unexpired().map(Cookie::from).collect()
    }

    fn get_all_any(&self) -> Vec<Cookie> {
        self.0.read().unwrap().iter_any().map(Cookie::from).collect()
    }
}

pub struct CookieStorePyProxy(pub Py<CookieStore>);

impl reqwest::cookie::CookieStore for CookieStorePyProxy {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &http::HeaderValue>, url: &url::Url) {
        let cookies = cookie_headers.filter_map(|val| {
            std::str::from_utf8(val.as_bytes())
                .map_err(cookie::ParseError::from)
                .and_then(cookie::Cookie::parse)
                .map(|c| c.into_owned())
                .ok()
        });

        self.0.get().0.write().unwrap().store_response_cookies(cookies, url);
    }

    fn cookies(&self, url: &url::Url) -> Option<http::HeaderValue> {
        let cookies_str = self
            .0
            .get()
            .0
            .read()
            .unwrap()
            .get_request_values(url)
            .map(|(name, value)| format!("{}={}", name, value))
            .collect::<Vec<_>>()
            .join("; ");

        if cookies_str.is_empty() {
            return None;
        }
        http::HeaderValue::from_maybe_shared(Bytes::from(cookies_str)).ok()
    }
}
