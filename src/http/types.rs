use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyDictItems, PyInt, PyList, PyMapping, PyString, PyTuple};
use pyo3::{Bound, FromPyObject, IntoPyObject, Py, PyAny, PyErr, PyResult, Python};
use pythonize::{depythonize, pythonize};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Serialize, Deserialize, Debug)]
pub struct Method(#[serde(with = "http_serde::method")] pub http::Method);
pub struct HeaderName(pub http::HeaderName);
pub struct HeaderValue(pub http::HeaderValue);
#[derive(Serialize, Deserialize, Debug)]
pub struct Version(#[serde(with = "http_serde::version")] pub http::Version);
#[derive(Debug)]
pub struct StatusCode(pub http::StatusCode);
#[derive(Serialize, Deserialize, Default)]
pub struct JsonValue(pub serde_json::Value);
#[derive(FromPyObject, IntoPyObject)]
pub struct Extensions(pub Py<PyDict>);
pub struct EncodablePairs(pub Vec<(String, JsonValue)>);

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

impl<'py> IntoPyObject<'py> for HeaderValue {
    type Target = PyString;
    type Output = Bound<'py, PyString>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(PyString::new(py, HeaderValue::str_res(&self.0)?))
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
    pub fn str_res(v: &http::HeaderValue) -> PyResult<&str> {
        v.to_str().map_err(|e| PyValueError::new_err(e.to_string()))
    }
}

impl<'py> FromPyObject<'py> for EncodablePairs {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(EncodablePairs(EncodableParams::extract(ob)?))
    }
}
#[derive(FromPyObject)]
enum EncodableParams<'py> {
    List(Bound<'py, PyList>),
    Tuple(Bound<'py, PyTuple>),
    DictItems(Bound<'py, PyDictItems>),
    Mapping(Bound<'py, PyMapping>),
}
impl EncodableParams<'_> {
    fn extract(ob: &Bound<PyAny>) -> PyResult<Vec<(String, JsonValue)>> {
        match ob.extract::<EncodableParams>()? {
            EncodableParams::List(v) => Self::extract_sized_iter(v.into_iter()),
            EncodableParams::Tuple(v) => Self::extract_sized_iter(v.into_iter()),
            EncodableParams::DictItems(v) => Self::extract_iter(v.len()?, v.try_iter()?),
            EncodableParams::Mapping(v) => Self::extract_sized_iter(v.items()?.into_iter()),
        }
    }

    fn extract_sized_iter<'py, I: ExactSizeIterator<Item = Bound<'py, PyAny>>>(
        iter: I,
    ) -> PyResult<Vec<(String, JsonValue)>> {
        let mut res: Vec<(String, JsonValue)> = Vec::with_capacity(iter.len());
        for item in iter {
            res.push(item.extract()?);
        }
        Ok(res)
    }

    fn extract_iter<'py, I: Iterator<Item = PyResult<Bound<'py, PyAny>>>>(
        len: usize,
        iter: I,
    ) -> PyResult<Vec<(String, JsonValue)>> {
        let mut res: Vec<(String, JsonValue)> = Vec::with_capacity(len);
        for item in iter {
            res.push(item?.extract()?);
        }
        Ok(res)
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
        depythonize(ob).map_err(|e| PyValueError::new_err(e.to_string()))
    }
}
impl From<reqwest::Version> for Version {
    fn from(version: reqwest::Version) -> Self {
        Version(version)
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
