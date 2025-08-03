use crate::http::EncodablePairs;
use pyo3::basic::CompareOp;
use pyo3::exceptions::PyValueError;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyMappingProxy, PyString};
use serde::Serialize;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::IpAddr;
use std::str::FromStr;

#[pyclass]
pub struct Url {
    url: url::Url,
    query: Option<Vec<(Py<PyString>, Py<PyString>)>>,
}

#[pymethods]
impl Url {
    #[new]
    fn py_new(url: UrlType) -> Self {
        Url::new(url.0)
    }

    #[staticmethod]
    fn parse(url: &str) -> PyResult<Self> {
        Ok(Url::new(url::Url::parse(url).map_err(|e| PyValueError::new_err(e.to_string()))?))
    }

    #[staticmethod]
    fn parse_with_params(url: &str, query: EncodablePairs) -> PyResult<Self> {
        let mut url = Url::parse(url)?;
        Self::extend_query_inner(&mut url.url, Some(query))?;
        Ok(url)
    }

    fn join(&self, input: &str) -> PyResult<Self> {
        let url = self.url.join(input).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Url::new(url))
    }

    fn make_relative(&self, base: &Self) -> Option<String> {
        self.url.make_relative(&base.url)
    }

    #[getter]
    fn origin_ascii(&self) -> String {
        self.url.origin().ascii_serialization()
    }

    #[getter]
    fn origin_unicode(&self) -> String {
        self.url.origin().unicode_serialization()
    }

    #[getter]
    fn scheme(&self) -> &str {
        self.url.scheme()
    }

    #[getter]
    fn is_special(&self) -> bool {
        self.url.is_special()
    }

    #[getter]
    fn has_authority(&self) -> bool {
        self.url.has_authority()
    }

    #[getter]
    fn authority(&self) -> &str {
        self.url.authority()
    }

    #[getter]
    fn cannot_be_a_base(&self) -> bool {
        self.url.cannot_be_a_base()
    }

    #[getter]
    fn username(&self) -> &str {
        self.url.username()
    }

    #[getter]
    fn password(&self) -> Option<&str> {
        self.url.password()
    }

    #[getter]
    fn has_host(&self) -> bool {
        self.url.has_host()
    }

    #[getter]
    fn host_str(&self) -> Option<&str> {
        self.url.host_str()
    }

    #[getter]
    fn domain(&self) -> Option<&str> {
        self.url.domain()
    }

    #[getter]
    fn port(&self) -> Option<u16> {
        self.url.port()
    }

    #[getter]
    fn port_or_known_default(&self) -> Option<u16> {
        self.url.port_or_known_default()
    }

    #[getter]
    fn path(&self) -> &str {
        self.url.path()
    }

    #[getter]
    fn path_segments<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyList>>> {
        self.url
            .path_segments()
            .map(|v| PyList::new(py, v.collect::<Vec<_>>()))
            .transpose()
    }

    #[getter]
    fn query_string(&self) -> Option<&str> {
        self.url.query()
    }

    #[getter]
    fn query_pairs<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        PyList::new(py, self.query_pairs_vec(py))
    }

    #[getter]
    fn query_dict_multi_value<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyMappingProxy>> {
        let dict = PyDict::new(py);
        for (k, v) in self.query_pairs_vec(py) {
            match dict.get_item(k)? {
                None => dict.set_item(k, PyList::new(py, [v])?)?,
                Some(existing) => existing.downcast_exact::<PyList>()?.append(v)?,
            }
        }
        Ok(PyMappingProxy::new(py, dict.as_mapping()))
    }

    #[getter]
    fn fragment(&self) -> Option<&str> {
        self.url.fragment()
    }

    fn with_fragment(&self, fragment: Option<&str>) -> Self {
        let mut url = self.url.clone();
        url.set_fragment(fragment);
        Url::new(url)
    }

    fn with_query(&self, query: Option<EncodablePairs>) -> PyResult<Self> {
        let mut url = self.url.clone();
        url.set_query(None);
        Self::extend_query_inner(&mut url, query)?;
        Ok(Url::new(url))
    }

    fn extend_query(&self, query: Option<EncodablePairs>) -> PyResult<Self> {
        let mut url = self.url.clone();
        Self::extend_query_inner(&mut url, query)?;
        Ok(Url::new(url))
    }

    fn with_query_string(&self, query: Option<&str>) -> Self {
        let mut url = self.url.clone();
        url.set_query(query);
        Url::new(url)
    }

    fn with_path(&self, path: &str) -> Self {
        let mut url = self.url.clone();
        url.set_path(path);
        Url::new(url)
    }

    fn with_path_segments(&self, segments: Vec<String>) -> PyResult<Self> {
        let mut url = self.url.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| PyValueError::new_err("cannot be base"))?;
            path.clear();
            path.extend(segments);
        }
        Ok(Url::new(url))
    }

    fn with_port(&self, port: Option<u16>) -> PyResult<Self> {
        let mut url = self.url.clone();
        url.set_port(port)
            .map_err(|_| PyValueError::new_err("cannot be base"))?;
        Ok(Url::new(url))
    }

    fn with_host(&self, host: Option<&str>) -> PyResult<Self> {
        let mut url = self.url.clone();
        url.set_host(host).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Url::new(url))
    }

    fn with_ip_host(&self, addr: &str) -> PyResult<Self> {
        let addr = IpAddr::from_str(addr).map_err(|e| PyValueError::new_err(e.to_string()))?;
        let mut url = self.url.clone();
        url.set_ip_host(addr)
            .map_err(|_| PyValueError::new_err("cannot be base"))?;
        Ok(Url::new(url))
    }

    fn with_username(&self, username: &str) -> PyResult<Self> {
        let mut url = self.url.clone();
        url.set_username(username)
            .map_err(|_| PyValueError::new_err("cannot be base"))?;
        Ok(Url::new(url))
    }

    fn with_password(&self, password: Option<&str>) -> PyResult<Self> {
        let mut url = self.url.clone();
        url.set_password(password)
            .map_err(|_| PyValueError::new_err("cannot be base"))?;
        Ok(Url::new(url))
    }

    fn with_scheme(&self, scheme: &str) -> PyResult<Self> {
        let mut url = self.url.clone();
        url.set_scheme(scheme)
            .map_err(|_| PyValueError::new_err("Invalid scheme"))?;
        Ok(Url::new(url))
    }

    fn copy(&self) -> Self {
        Url::new(self.url.clone())
    }

    fn __copy__(&self) -> Self {
        Url::new(self.url.clone())
    }

    fn __truediv__(&self, other: &str) -> PyResult<Self> {
        self.join(other)
    }

    fn __str__(&self) -> &str {
        self.url.as_str()
    }

    fn __repr__(slf: Bound<Self>) -> PyResult<String> {
        let url_repr = slf.call_method0(intern!(slf.py(), "__str__"))?.repr()?;
        Ok(format!("Url({})", url_repr.to_str()?))
    }

    fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.url.hash(&mut hasher);
        hasher.finish()
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp) -> bool {
        match op {
            CompareOp::Lt => self.url < other.url,
            CompareOp::Le => self.url <= other.url,
            CompareOp::Eq => self.url == other.url,
            CompareOp::Ne => self.url != other.url,
            CompareOp::Gt => self.url > other.url,
            CompareOp::Ge => self.url >= other.url,
        }
    }
}
impl Url {
    fn new(url: url::Url) -> Self {
        Url { url, query: None }
    }

    fn query_pairs_vec(&mut self, py: Python) -> &Vec<(Py<PyString>, Py<PyString>)> {
        if self.query.is_none() {
            self.query = Some(
                self.url
                    .query_pairs()
                    .map(|(k, v)| {
                        let k = PyString::new(py, &k);
                        let v = PyString::new(py, &v);
                        (k.unbind(), v.unbind())
                    })
                    .collect(),
            );
        }
        self.query.as_ref().unwrap()
    }

    fn extend_query_inner(url: &mut url::Url, query: Option<EncodablePairs>) -> PyResult<()> {
        if let Some(query) = query.map(|q| q.0) {
            let mut url_query = url.query_pairs_mut();
            let serializer = serde_urlencoded::Serializer::new(&mut url_query);
            query
                .serialize(serializer)
                .map_err(|e| PyValueError::new_err(e.to_string()))?;
        }
        Ok(())
    }
}
impl From<reqwest::Url> for Url {
    fn from(value: reqwest::Url) -> Self {
        Url::new(value)
    }
}
impl From<UrlType> for Url {
    fn from(value: UrlType) -> Self {
        Url::new(value.0)
    }
}

#[derive(Clone)]
pub struct UrlType(pub url::Url);
impl<'py> FromPyObject<'py> for UrlType {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        if let Ok(url) = ob.downcast_exact::<Url>() {
            Ok(UrlType(url.borrow().url.clone()))
        } else {
            let url =
                url::Url::parse(ob.str()?.extract::<&str>()?).map_err(|e| PyValueError::new_err(e.to_string()))?;
            Ok(UrlType(url))
        }
    }
}
