use crate::http_types::{Extensions, HeaderMapExt, StatusCodeExt, VersionExt};
use pyo3::exceptions::PyValueError;
use pyo3::{Bound, FromPyObject, PyAny, PyErr, PyResult};
use pythonize::depythonize;
use serde::Deserialize;

#[derive(Deserialize, Default)]
pub struct ResponseDict {
    status_code: Option<StatusCodeExt>,
    headers: Option<HeaderMapExt>,
    version: Option<VersionExt>,
    body_bytes: Option<Vec<u8>>,
    extensions: Option<Extensions>,
}
impl<'py> FromPyObject<'py> for ResponseDict {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(depythonize(ob)?)
    }
}
impl TryInto<reqwest::Response> for ResponseDict {
    type Error = PyErr;
    fn try_into(mut self) -> PyResult<reqwest::Response> {
        let mut res = http::Response::builder();
        if let Some(status_code) = self.status_code.take() {
            res = res.status(status_code.0);
        }
        if let Some(headers) = self.headers.take() {
            res.headers_mut().map(|h| *h = headers.0);
        }
        if let Some(version) = self.version.take() {
            res = res.version(version.0);
        }
        if let Some(extensions) = self.extensions.take() {
            res = res.extension(extensions);
        }
        let res = res
            .body(self.body_bytes.take().unwrap_or_default())
            .map_err(|e| PyValueError::new_err(format!("Failed to build response: {}", e)))?;
        Ok(reqwest::Response::from(res))
    }
}
