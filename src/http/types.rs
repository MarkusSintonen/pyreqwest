use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::PyType;
use pyo3::{Bound, FromPyObject, IntoPyObject, Py, PyAny, PyErr, PyResult, Python};
use pythonize::{depythonize, pythonize};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct Method(#[serde(with = "http_serde::method")] pub http::Method);
#[derive(Serialize, Deserialize, Clone)]
pub struct HeaderMap(#[serde(with = "http_serde::header_map")] pub http::HeaderMap);
#[derive(Serialize, Deserialize, Clone)]
pub struct Version(#[serde(with = "http_serde::version")] pub http::Version);
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Extensions(pub serde_json::Map<String, serde_json::Value>);
#[derive(Serialize, Deserialize, Clone)]
pub struct StatusCode(#[serde(with = "http_serde::status_code")] pub http::StatusCode);
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct JsonValue(pub serde_json::Value);

impl<'py> IntoPyObject<'py> for Method {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(pythonize(py, &self)?)
    }
}
impl<'py> FromPyObject<'py> for Method {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(depythonize(ob)?)
    }
}
impl From<reqwest::Method> for Method {
    fn from(method: reqwest::Method) -> Self {
        Method(method)
    }
}

impl<'py> IntoPyObject<'py> for Version {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(pythonize(py, &self)?)
    }
}
impl<'py> FromPyObject<'py> for Version {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(depythonize(ob)?)
    }
}
impl From<reqwest::Version> for Version {
    fn from(version: reqwest::Version) -> Self {
        Version(version)
    }
}

impl<'py> IntoPyObject<'py> for HeaderMap {
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
impl<'py> FromPyObject<'py> for HeaderMap {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(depythonize(ob)?)
    }
}
impl From<http::HeaderMap> for HeaderMap {
    fn from(header_map: http::HeaderMap) -> Self {
        HeaderMap(header_map)
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

impl<'py> IntoPyObject<'py> for StatusCode {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(pythonize(py, &self)?)
    }
}
impl<'py> FromPyObject<'py> for StatusCode {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(depythonize(ob)?)
    }
}
impl From<http::StatusCode> for StatusCode {
    fn from(status: http::StatusCode) -> Self {
        StatusCode(status)
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
