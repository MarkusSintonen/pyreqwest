use crate::http::utils::KeyValPairs;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyInt, PyString};
use pyo3::{Bound, FromPyObject, IntoPyObject, Py, PyAny, PyErr, PyResult, Python, intern};
use pythonize::{depythonize, pythonize};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::str::FromStr;

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Method(pub http::Method);
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct HeaderName(pub http::HeaderName);
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct HeaderValue(pub http::HeaderValue);
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Version(pub http::Version);
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct StatusCode(pub http::StatusCode);
#[derive(Serialize, Deserialize)]
pub struct JsonValue(pub serde_json::Value);
pub struct Extensions(pub Py<PyDict>);
pub struct QueryParams(pub Vec<(String, JsonValue)>);
pub struct FormParams(pub Vec<(String, JsonValue)>);

impl<'py> IntoPyObject<'py> for Method {
    type Target = PyString;
    type Output = Bound<'py, PyString>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(PyString::new(py, self.0.as_str()))
    }
}
impl<'py> FromPyObject<'py> for Method {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        let method = ob
            .extract::<&str>()?
            .parse::<http::Method>()
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Method(method))
    }
}
impl From<reqwest::Method> for Method {
    fn from(method: reqwest::Method) -> Self {
        Method(method)
    }
}

impl<'py> IntoPyObject<'py> for HeaderName {
    type Target = PyString;
    type Output = Bound<'py, PyString>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(PyString::new(py, self.0.as_str()))
    }
}
impl<'py> FromPyObject<'py> for HeaderName {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        let val = ob.extract::<&str>()?;
        let val = http::HeaderName::from_str(val).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(HeaderName(val))
    }
}
impl Ord for HeaderName {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_str().cmp(other.0.as_str())
    }
}
impl PartialOrd for HeaderName {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'py> IntoPyObject<'py> for HeaderValue {
    type Target = PyString;
    type Output = Bound<'py, PyString>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(PyString::new(py, &HeaderValue::inner_str(&self.0)))
    }
}
impl<'py> FromPyObject<'py> for HeaderValue {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        let val = ob.extract::<&str>()?;
        let val = http::HeaderValue::from_str(val).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(HeaderValue(val))
    }
}
impl HeaderValue {
    pub fn inner_str(v: &http::HeaderValue) -> Cow<'_, str> {
        match v.to_str() {
            Ok(s) => Cow::Borrowed(s),
            Err(_) => String::from_utf8_lossy(v.as_bytes()),
        }
    }
}
impl TryFrom<&str> for HeaderValue {
    type Error = PyErr;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let val = http::HeaderValue::from_str(value).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(HeaderValue(val))
    }
}
impl Ord for HeaderValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_bytes().cmp(other.0.as_bytes())
    }
}
impl PartialOrd for HeaderValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'py> FromPyObject<'py> for QueryParams {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(QueryParams(ob.extract::<KeyValPairs>()?.into_vec()?))
    }
}
impl<'py> FromPyObject<'py> for FormParams {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(FormParams(ob.extract::<KeyValPairs>()?.into_vec()?))
    }
}

impl<'py> IntoPyObject<'py> for Version {
    type Target = PyString;
    type Output = Bound<'py, PyString>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        if self.0 == http::Version::HTTP_10 {
            Ok(intern!(py, "HTTP/1.0").clone())
        } else if self.0 == http::Version::HTTP_11 {
            Ok(intern!(py, "HTTP/1.1").clone())
        } else if self.0 == http::Version::HTTP_2 {
            Ok(intern!(py, "HTTP/2.0").clone())
        } else if self.0 == http::Version::HTTP_3 {
            Ok(intern!(py, "HTTP/3.0").clone())
        } else if self.0 == http::Version::HTTP_09 {
            Ok(intern!(py, "HTTP/0.9").clone())
        } else {
            Err(PyValueError::new_err("invalid http version"))
        }
    }
}
impl<'py> FromPyObject<'py> for Version {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(match ob.extract::<&str>()? {
            "HTTP/1.0" => Version(http::Version::HTTP_10),
            "HTTP/1.1" => Version(http::Version::HTTP_11),
            "HTTP/2.0" => Version(http::Version::HTTP_2),
            "HTTP/3.0" => Version(http::Version::HTTP_3),
            "HTTP/0.9" => Version(http::Version::HTTP_09),
            _ => Err(PyValueError::new_err("invalid http version"))?,
        })
    }
}
impl From<reqwest::Version> for Version {
    fn from(version: reqwest::Version) -> Self {
        Version(version)
    }
}

impl<'py> FromPyObject<'py> for Extensions {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        if let Ok(dict) = ob.downcast_exact::<PyDict>() {
            Ok(Extensions(dict.copy()?.unbind()))
        } else {
            let dict = PyDict::new(ob.py());
            ob.extract::<KeyValPairs>()?
                .for_each(|(key, value): (Bound<'py, PyString>, Bound<'py, PyAny>)| dict.set_item(key, value))?;
            Ok(Extensions(dict.unbind()))
        }
    }
}
impl Extensions {
    pub fn copy(&self, py: Python) -> PyResult<Extensions> {
        Ok(Extensions(self.0.bind(py).copy()?.unbind()))
    }

    pub fn into_response(self, response_extensions: &mut http::Extensions) -> PyResult<()> {
        Python::with_gil(|py| {
            let result_ext = self.0.into_bound(py);
            if let Some(resp_ext) = response_extensions.remove::<Extensions>() {
                let resp_ext = resp_ext.0.into_bound(py);
                result_ext.update(resp_ext.as_mapping())?;
            }
            response_extensions.insert(Extensions(result_ext.unbind()));
            Ok(())
        })
    }
}
impl Clone for Extensions {
    fn clone(&self) -> Self {
        Extensions(Python::with_gil(|py| self.0.clone_ref(py)))
    }
}

impl<'py> IntoPyObject<'py> for StatusCode {
    type Target = PyInt;
    type Output = Bound<'py, PyInt>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(PyInt::new(py, self.0.as_u16()))
    }
}
impl<'py> FromPyObject<'py> for StatusCode {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        let status = http::StatusCode::from_u16(ob.extract::<u16>()?)
            .map_err(|_| PyValueError::new_err("invalid status code"))?;
        Ok(StatusCode(status))
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
