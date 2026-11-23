use crate::internal::utils::KeyValPairs;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyInt, PyString};
use pyo3::{Bound, FromPyObject, IntoPyObject, Py, PyAny, PyErr, PyResult, Python, intern};
use pythonize::{depythonize, pythonize};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::str::FromStr;

#[derive(FromPyObject, Serialize)]
#[serde(untagged)]
pub enum QueryVal {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}
#[derive(FromPyObject, Serialize)]
#[serde(untagged)]
pub enum QueryValue {
    Value(QueryVal),
    Array(Vec<QueryVal>),
}

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
pub struct QueryParams(pub Vec<(String, QueryVal)>);
pub struct FormParams(pub Vec<(String, QueryVal)>);

impl<'py> IntoPyObject<'py> for Method {
    type Target = PyString;
    type Output = Bound<'py, PyString>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(PyString::new(py, self.0.as_str()))
    }
}
impl<'py> FromPyObject<'_, 'py> for Method {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        let method = obj
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
impl<'py> FromPyObject<'_, 'py> for HeaderName {
    type Error = PyErr;
    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        let val = obj.extract::<&str>()?;
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
        Ok(PyString::new(py, &HeaderValue::inner_str(&self.0)?))
    }
}
impl<'py> FromPyObject<'_, 'py> for HeaderValue {
    type Error = PyErr;
    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        let val = obj.extract::<&str>()?;
        let val = http::HeaderValue::from_str(val).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(HeaderValue(val))
    }
}
impl HeaderValue {
    pub fn inner_str(v: &http::HeaderValue) -> PyResult<Cow<'_, str>> {
        let v = v.to_str().map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Cow::Borrowed(v))
    }
}
impl FromStr for HeaderValue {
    type Err = PyErr;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let val = http::HeaderValue::from_str(value).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(HeaderValue(val))
    }
}
impl TryFrom<&str> for HeaderValue {
    type Error = PyErr;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(HeaderValue(value.parse::<HeaderValue>()?.0))
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

impl<'py> FromPyObject<'_, 'py> for QueryParams {
    type Error = PyErr;
    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        Ok(QueryParams(extract_query_pairs(obj, "query")?))
    }
}
impl<'py> FromPyObject<'_, 'py> for FormParams {
    type Error = PyErr;
    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        Ok(FormParams(extract_query_pairs(obj, "form")?))
    }
}
/// Flattens list values into repeated pairs (`("a", [1, 2])` -> `a=1&a=2`), which serde_urlencoded can serialize.
fn extract_query_pairs(obj: Borrowed<'_, '_, PyAny>, ctx: &str) -> PyResult<Vec<(String, QueryVal)>> {
    let params: Vec<(String, QueryValue)> = obj.extract::<KeyValPairs>()?.into_vec(ctx)?;
    let mut pairs = Vec::with_capacity(params.len());
    for (key, val) in params {
        match val {
            QueryValue::Array(arr) => pairs.extend(arr.into_iter().map(|v| (key.clone(), v))),
            QueryValue::Value(v) => pairs.push((key, v)),
        }
    }
    Ok(pairs)
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
impl<'py> FromPyObject<'_, 'py> for Version {
    type Error = PyErr;
    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        Ok(match obj.extract::<&str>()? {
            "HTTP/1.0" => Version(http::Version::HTTP_10),
            "HTTP/1.1" => Version(http::Version::HTTP_11),
            "HTTP/2.0" => Version(http::Version::HTTP_2),
            "HTTP/3.0" => Version(http::Version::HTTP_3),
            "HTTP/0.9" => Version(http::Version::HTTP_09),
            _ => Err(PyValueError::new_err("invalid http version"))?,
        })
    }
}

impl<'py> FromPyObject<'_, 'py> for Extensions {
    type Error = PyErr;
    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        if let Ok(dict) = obj.cast_exact::<PyDict>() {
            Ok(Extensions(dict.copy()?.unbind()))
        } else {
            let dict = PyDict::new(obj.py());
            obj.extract::<KeyValPairs>()?
                .for_each("extensions", |(key, value): (Bound<'py, PyString>, Bound<'py, PyAny>)| {
                    dict.set_item(key, value)
                })?;
            Ok(Extensions(dict.unbind()))
        }
    }
}
impl Extensions {
    pub fn copy(&self, py: Python) -> PyResult<Extensions> {
        Ok(Extensions(self.0.bind(py).copy()?.unbind()))
    }
}
impl Clone for Extensions {
    fn clone(&self) -> Self {
        Extensions(Python::attach(|py| self.0.clone_ref(py)))
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
impl<'py> FromPyObject<'_, 'py> for StatusCode {
    type Error = PyErr;
    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        let status = http::StatusCode::from_u16(obj.extract::<u16>()?)
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
impl<'py> FromPyObject<'_, 'py> for JsonValue {
    type Error = PyErr;
    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        Ok(depythonize(&obj)?)
    }
}
