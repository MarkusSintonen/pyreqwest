use pyo3::intern;
use pyo3::prelude::*;

pub struct CookieStore(Py<PyAny>);

impl CookieStore {
    pub fn new(provider: Py<PyAny>) -> Self {
        Self(provider)
    }
}

impl reqwest::cookie::CookieStore for CookieStore {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &reqwest::header::HeaderValue>, url: &reqwest::Url) {
        let headers = cookie_headers
            .map(|val| val.to_str().unwrap().to_string())
            .collect::<Vec<_>>();
        let url_str = url.as_str();

        Python::attach(|py| {
            self.0
                .bind(py)
                .call_method1(intern!(py, "set_cookies"), (headers, url_str))
                .unwrap(); // No better way to handle Err than RequestPanicError
        });
    }

    fn cookies(&self, url: &reqwest::Url) -> Option<reqwest::header::HeaderValue> {
        let url_str = url.as_str();

        Python::attach(|py| {
            let cookie_val = self
                .0
                .bind(py)
                .call_method1(intern!(py, "cookies"), (url_str,))
                .unwrap() // No better way to handle Err than RequestPanicError
                .extract::<Option<String>>()
                .unwrap();

            if let Some(cookie_str) = cookie_val {
                if !cookie_str.is_empty() {
                    return Some(reqwest::header::HeaderValue::from_str(&cookie_str).unwrap());
                }
            }
            None
        })
    }
}
