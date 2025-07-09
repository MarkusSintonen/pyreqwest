use http::HeaderMap;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::PyType;
use pyo3::{Bound, FromPyObject, IntoPyObject, Py, PyAny, PyErr, PyResult, Python};
use pythonize::{depythonize, pythonize};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct UrlExt(#[serde(with = "http_serde::uri")] pub http::Uri);
#[derive(Serialize, Deserialize, Clone)]
pub struct MethodExt(#[serde(with = "http_serde::method")] pub http::Method);
#[derive(Serialize, Deserialize, Clone)]
pub struct HeaderMapExt(#[serde(with = "http_serde::header_map")] pub HeaderMap);
#[derive(Serialize, Deserialize, Clone)]
pub struct VersionExt(#[serde(with = "http_serde::version")] pub http::Version);
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Extensions(pub serde_json::Map<String, serde_json::Value>);
#[derive(Serialize, Deserialize, Clone)]
pub struct StatusCodeExt(#[serde(with = "http_serde::status_code")] pub http::StatusCode);
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct JsonValue(pub serde_json::Value);

impl<'py> IntoPyObject<'py> for UrlExt {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(pythonize(py, &self)?)
    }
}
impl<'py> FromPyObject<'py> for UrlExt {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(depythonize(ob)?)
    }
}
impl TryFrom<reqwest::Url> for UrlExt {
    type Error = PyErr;
    fn try_from(value: reqwest::Url) -> PyResult<Self> {
        Ok(UrlExt(
            value
                .as_str()
                .parse()
                .map_err(|e| PyValueError::new_err(format!("Invalid URL format: {}", e)))?,
        ))
    }
}
impl TryInto<reqwest::Url> for UrlExt {
    type Error = PyErr;
    fn try_into(self) -> PyResult<reqwest::Url> {
        self.0
            .to_string()
            .parse::<reqwest::Url>()
            .map_err(|e| PyValueError::new_err(format!("Invalid URL format: {}", e)))
    }
}

impl<'py> IntoPyObject<'py> for MethodExt {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(pythonize(py, &self)?)
    }
}
impl<'py> FromPyObject<'py> for MethodExt {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(depythonize(ob)?)
    }
}
impl From<reqwest::Method> for MethodExt {
    fn from(method: reqwest::Method) -> Self {
        MethodExt(method)
    }
}

impl<'py> IntoPyObject<'py> for VersionExt {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(pythonize(py, &self)?)
    }
}
impl<'py> FromPyObject<'py> for VersionExt {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(depythonize(ob)?)
    }
}
impl From<reqwest::Version> for VersionExt {
    fn from(version: reqwest::Version) -> Self {
        VersionExt(version)
    }
}

impl<'py> IntoPyObject<'py> for HeaderMapExt {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let dict = multidict(py)?;
        for (key, value) in self.0.iter() {
            dict.set_item(key.as_str(), value.as_bytes())?;
        }
        Ok(dict)
    }
}
impl<'py> FromPyObject<'py> for HeaderMapExt {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(depythonize(ob)?)
    }
}
impl From<HeaderMap> for HeaderMapExt {
    fn from(header_map: HeaderMap) -> Self {
        HeaderMapExt(header_map)
    }
}

impl<'py> IntoPyObject<'py> for Extensions {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(pythonize(py, &self)?)
    }
}
impl<'py> FromPyObject<'py> for Extensions {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(depythonize(ob)?)
    }
}
impl From<&http::Extensions> for Extensions {
    fn from(http_extensions: &http::Extensions) -> Self {
        match http_extensions.get::<Extensions>() {
            Some(ext) => Extensions(ext.0.clone()),
            None => Extensions(serde_json::Map::new()),
        }
    }
}

impl<'py> IntoPyObject<'py> for StatusCodeExt {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(pythonize(py, &self)?)
    }
}
impl<'py> FromPyObject<'py> for StatusCodeExt {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(depythonize(ob)?)
    }
}
impl From<http::StatusCode> for StatusCodeExt {
    fn from(status: http::StatusCode) -> Self {
        StatusCodeExt(status)
    }
}

impl<'py> FromPyObject<'py> for JsonValue {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(depythonize(ob)?)
    }
}

fn multidict(py: Python) -> PyResult<Bound<PyAny>> {
    static MULTIDICT_CELL: GILOnceCell<Py<PyType>> = GILOnceCell::new();
    MULTIDICT_CELL.import(py, "multidict", "CIMultiDict")?.call0()
}
