use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyDict, PyMapping, PyType};
use pyo3::{Bound, FromPyObject, IntoPyObject, Py, PyAny, PyErr, PyResult, Python};
use pythonize::{depythonize, pythonize};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Method(#[serde(with = "http_serde::method")] pub http::Method);
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HeaderMap(#[serde(with = "http_serde::header_map")] pub http::HeaderMap);
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Version(#[serde(with = "http_serde::version")] pub http::Version);
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StatusCode(#[serde(with = "http_serde::status_code")] pub http::StatusCode);
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct JsonValue(pub serde_json::Value);
#[derive(FromPyObject, IntoPyObject)]
pub struct Extensions(pub Py<PyDict>);

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
            let value = value
                .to_str()
                .map_err(|e| PyValueError::new_err(format!("Invalid header value: {}", e)))?;
            dict.set_item(key.as_str(), value)?;
        }
        Ok(dict)
    }
}
impl<'py> FromPyObject<'py> for HeaderMap {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        let mut headers = http::HeaderMap::new();
        for item in ob.downcast::<PyMapping>()?.items()?.try_iter()? {
            let tup: (String, String) = item?.extract()?;
            let name = http::HeaderName::from_str(&tup.0).map_err(|e| PyValueError::new_err(e.to_string()))?;
            let value = http::HeaderValue::from_str(&tup.1).map_err(|e| PyValueError::new_err(e.to_string()))?;
            headers.append(name, value);
        }
        Ok(HeaderMap::from(headers))
    }
}
impl From<http::HeaderMap> for HeaderMap {
    fn from(header_map: http::HeaderMap) -> Self {
        HeaderMap(header_map)
    }
}

impl Extensions {
    pub fn copy_dict(&self, py: Python) -> PyResult<Extensions> {
        Ok(Extensions(self.0.bind(py).copy()?.unbind()))
    }
}
impl Clone for Extensions {
    fn clone(&self) -> Self {
        Extensions(Python::with_gil(|py| self.0.clone_ref(py)))
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

impl<'py> IntoPyObject<'py> for JsonValue {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(pythonize(py, &self)?)
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
