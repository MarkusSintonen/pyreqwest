use crate::cookie::{Cookie, CookieType};
use crate::http::UrlType;
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

    fn matches(&self, url: UrlType) -> PyResult<Vec<Cookie>> {
        Ok(self
            .lock_read()?
            .matches(&url.0)
            .into_iter()
            .map(Cookie::from)
            .collect())
    }

    fn contains(&self, domain: &str, path: &str, name: &str) -> PyResult<bool> {
        Ok(self.lock_read()?.contains(domain, path, name))
    }

    fn contains_any(&self, domain: &str, path: &str, name: &str) -> PyResult<bool> {
        Ok(self.lock_read()?.contains_any(domain, path, name))
    }

    fn get(&self, domain: &str, path: &str, name: &str) -> PyResult<Option<Cookie>> {
        Ok(self.lock_read()?.get(domain, path, name).map(Cookie::from))
    }

    fn get_any(&self, domain: &str, path: &str, name: &str) -> PyResult<Option<Cookie>> {
        Ok(self.lock_read()?.get_any(domain, path, name).map(Cookie::from))
    }

    fn remove(&self, domain: &str, path: &str, name: &str) -> PyResult<Option<Cookie>> {
        Ok(self.lock_write()?.remove(domain, path, name).map(Cookie::from))
    }

    fn insert(&self, cookie: CookieType, request_url: UrlType) -> PyResult<()> {
        self.lock_write()?
            .insert_raw(&cookie.0, &request_url.0)
            .map_err(|e| PyValueError::new_err(e.to_string()))
            .map(|_| ())
    }

    fn clear(&self) -> PyResult<()> {
        self.lock_write()?.clear();
        Ok(())
    }

    fn get_all_unexpired(&self) -> PyResult<Vec<Cookie>> {
        Ok(self.lock_read()?.iter_unexpired().map(Cookie::from).collect())
    }

    fn get_all_any(&self) -> PyResult<Vec<Cookie>> {
        Ok(self.lock_read()?.iter_any().map(Cookie::from).collect())
    }
}
impl CookieStore {
    fn lock_read(&self) -> PyResult<std::sync::RwLockReadGuard<'_, cookie_store::CookieStore>> {
        self.0.read().map_err(|e| PyValueError::new_err(e.to_string()))
    }

    fn lock_write(&self) -> PyResult<std::sync::RwLockWriteGuard<'_, cookie_store::CookieStore>> {
        self.0.write().map_err(|e| PyValueError::new_err(e.to_string()))
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

        #[allow(clippy::unwrap_used)] // Trait does not allow returning a Result
        let mut store = self.0.get().0.write().unwrap();
        store.store_response_cookies(cookies, url);
    }

    #[allow(clippy::unwrap_in_result)]
    fn cookies(&self, url: &url::Url) -> Option<http::HeaderValue> {
        let cookies_str = {
            #[allow(clippy::unwrap_used)] // Trait does not allow returning a Result
            let store = self.0.get().0.read().unwrap();
            store
                .get_request_values(url)
                .map(|(name, value)| format!("{}={}", name, value))
                .collect::<Vec<_>>()
                .join("; ")
        };

        if cookies_str.is_empty() {
            return None;
        }
        http::HeaderValue::from_maybe_shared(Bytes::from(cookies_str)).ok()
    }
}
