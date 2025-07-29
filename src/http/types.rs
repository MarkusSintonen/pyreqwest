use pyo3::call::PyCallArgs;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyDict, PyDictItems, PyInt, PyList, PyMapping, PyTuple, PyType};
use pyo3::{Bound, FromPyObject, IntoPyObject, Py, PyAny, PyErr, PyResult, Python};
use pythonize::{depythonize, pythonize};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::str::FromStr;

#[derive(Serialize, Deserialize, Debug)]
pub struct Method(#[serde(with = "http_serde::method")] pub http::Method);
#[derive(Clone)]
pub struct HeaderMap(pub http::HeaderMap);
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
pub struct MultiDictProxy(Py<PyMapping>);
pub struct CIMultiDict(Py<PyMapping>);

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

impl<'py> FromPyObject<'py> for HeaderName {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        let val = ob.extract::<&str>()?;
        let val = http::HeaderName::from_str(val).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(HeaderName(val))
    }
}

impl<'py> FromPyObject<'py> for HeaderValue {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        let val = ob.extract::<&str>()?;
        let val = http::HeaderValue::from_str(val).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(HeaderValue(val))
    }
}

impl<'py> IntoPyObject<'py> for HeaderMap {
    type Target = PyMapping;
    type Output = Bound<'py, PyMapping>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(CIMultiDict::new(py, &self.0)?.0.into_bound(py))
    }
}
impl<'py> FromPyObject<'py> for HeaderMap {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        let mut headers = http::HeaderMap::new();
        for item in ob.downcast::<PyMapping>()?.items()?.try_iter()? {
            let tup: (HeaderName, HeaderValue) = item?.extract()?;
            headers.append(tup.0.0, tup.1.0);
        }
        Ok(HeaderMap(headers))
    }
}
impl From<http::HeaderMap> for HeaderMap {
    fn from(header_map: http::HeaderMap) -> Self {
        HeaderMap(header_map)
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

impl<'py> IntoPyObject<'py> for MultiDictProxy {
    type Target = PyMapping;
    type Output = Bound<'py, PyMapping>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(self.0.into_bound(py))
    }
}
impl<'py> FromPyObject<'py> for MultiDictProxy {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        if ob.is_exact_instance(multi_dict_proxy_type(ob.py())?) {
            // Safety: We know that `ob` is a `MultiDictProxy` type from the check above
            let ob = unsafe { ob.downcast_unchecked::<PyMapping>() };
            Ok(MultiDictProxy(ob.as_unbound().clone_ref(ob.py())))
        } else if let Ok(map) = ob.downcast::<PyMapping>() {
            Ok(MultiDictProxy(multi_dict_proxy(ob.py(), (map,))?.unbind()))
        } else {
            Err(PyTypeError::new_err("Expected a CIMultiDict or Mapping"))
        }
    }
}
impl MultiDictProxy {
    pub fn new(py: Python, items: Vec<(Cow<'_, str>, Cow<'_, str>)>) -> PyResult<Self> {
        Ok(MultiDictProxy(multi_dict_proxy(py, (items,))?.unbind()))
    }
}

impl<'py> IntoPyObject<'py> for CIMultiDict {
    type Target = PyMapping;
    type Output = Bound<'py, PyMapping>;
    type Error = PyErr;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(self.0.into_bound(py))
    }
}
impl<'py> FromPyObject<'py> for CIMultiDict {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        if ob.is_exact_instance(ci_multi_dict_type(ob.py())?) {
            // Safety: We know that `ob` is a `CIMultiDict` type from the check above
            let ob = unsafe { ob.downcast_unchecked::<PyMapping>() };
            Ok(CIMultiDict(ob.as_unbound().clone_ref(ob.py())))
        } else if let Ok(map) = ob.downcast::<PyMapping>() {
            Ok(CIMultiDict(ci_multi_dict(ob.py(), (map,))?.unbind()))
        } else {
            Err(PyTypeError::new_err("Expected a CIMultiDict or Mapping"))
        }
    }
}
impl CIMultiDict {
    pub fn new(py: Python, headers: &http::HeaderMap) -> PyResult<Self> {
        let kv = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.to_str().unwrap_or("")))
            .collect::<Vec<_>>();
        Ok(CIMultiDict(ci_multi_dict(py, (kv,))?.unbind()))
    }

    pub fn to_http_header_map(&self, py: Python) -> PyResult<http::HeaderMap> {
        let mut header_map = http::HeaderMap::new();
        for item in self.0.bind(py).items()? {
            let (key, value): (HeaderName, HeaderValue) = item.extract()?;
            header_map.append(key.0, value.0);
        }
        Ok(header_map)
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

fn ci_multi_dict_type(py: Python) -> PyResult<&Bound<PyType>> {
    static MULTIDICT_CELL: GILOnceCell<Py<PyType>> = GILOnceCell::new();
    MULTIDICT_CELL.import(py, "multidict", "CIMultiDict")
}

fn ci_multi_dict<'py, A: PyCallArgs<'py>>(py: Python<'py>, args: A) -> PyResult<Bound<'py, PyMapping>> {
    let dict = ci_multi_dict_type(py)?.call1(args)?;
    // Safety: We know that `ob` is a `CIMultiDict` type from the call to `ci_multi_dict`
    let dict = unsafe { dict.downcast_into_unchecked::<PyMapping>() };
    Ok(dict)
}

fn multi_dict_type(py: Python) -> PyResult<&Bound<PyType>> {
    static MULTIDICT_CELL: GILOnceCell<Py<PyType>> = GILOnceCell::new();
    MULTIDICT_CELL.import(py, "multidict", "MultiDict")
}

fn multi_dict<'py, A: PyCallArgs<'py>>(py: Python<'py>, args: A) -> PyResult<Bound<'py, PyMapping>> {
    let dict = multi_dict_type(py)?.call1(args)?;
    // Safety: We know that `ob` is a `MultiDict` type from the call to `ci_multi_dict`
    let dict = unsafe { dict.downcast_into_unchecked::<PyMapping>() };
    Ok(dict)
}

fn multi_dict_proxy_type(py: Python) -> PyResult<&Bound<PyType>> {
    static MULTIDICT_CELL: GILOnceCell<Py<PyType>> = GILOnceCell::new();
    MULTIDICT_CELL.import(py, "multidict", "MultiDictProxy")
}

fn multi_dict_proxy<'py, A: PyCallArgs<'py>>(py: Python<'py>, args: A) -> PyResult<Bound<'py, PyMapping>> {
    let dict = multi_dict(py, args)?;
    let dict = multi_dict_proxy_type(py)?.call1((dict,))?;
    // Safety: We know that `ob` is a `MultiDict` type from the call to `ci_multi_dict`
    let dict = unsafe { dict.downcast_into_unchecked::<PyMapping>() };
    Ok(dict)
}
