use pyo3::basic::CompareOp;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::IpAddr;
use std::str::FromStr;
use pyo3::intern;
use pyo3::types::PyTuple;
use serde::Serialize;
use crate::http::{EncodablePairs, MultiDictProxy};

#[derive(Clone)]
#[pyclass]
pub struct Url(pub url::Url);

#[pymethods]
impl Url {
    #[new]
    fn new(url: UrlType) -> Self {
        Url(url.0)
    }

    #[staticmethod]
    fn parse(url: &str) -> PyResult<Self> {
        Ok(Url(url::Url::parse(url).map_err(|e| PyValueError::new_err(e.to_string()))?))
    }

    #[staticmethod]
    fn parse_with_params(url: &str, query: EncodablePairs) -> PyResult<Self> {
        let mut url = Url::parse(url)?;
        Self::extend_query_inner(&mut url.0, Some(query))?;
        Ok(url)
    }

    fn join(&self, input: &str) -> PyResult<Self> {
        let url = self.0.join(input).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Url(url))
    }

    fn make_relative(&self, base: &Self) -> Option<String> {
        self.0.make_relative(&base.0)
    }

    #[getter]
    fn origin_ascii(&self) -> String {
        self.0.origin().ascii_serialization()
    }

    #[getter]
    fn origin_unicode(&self) -> String {
        self.0.origin().unicode_serialization()
    }

    #[getter]
    fn scheme(&self) -> &str {
        self.0.scheme()
    }

    #[getter]
    fn is_special(&self) -> bool {
        self.0.is_special()
    }

    #[getter]
    fn has_authority(&self) -> bool {
        self.0.has_authority()
    }

    #[getter]
    fn authority(&self) -> &str {
        self.0.authority()
    }

    #[getter]
    fn cannot_be_a_base(&self) -> bool {
        self.0.cannot_be_a_base()
    }

    #[getter]
    fn username(&self) -> &str {
        self.0.username()
    }

    #[getter]
    fn password(&self) -> Option<&str> {
        self.0.password()
    }

    #[getter]
    fn has_host(&self) -> bool {
        self.0.has_host()
    }

    #[getter]
    fn host_str(&self) -> Option<&str> {
        self.0.host_str()
    }

    #[getter]
    fn domain(&self) -> Option<&str> {
        self.0.domain()
    }

    #[getter]
    fn port(&self) -> Option<u16> {
        self.0.port()
    }

    #[getter]
    fn port_or_known_default(&self) -> Option<u16> {
        self.0.port_or_known_default()
    }

    #[getter]
    fn path(&self) -> &str {
        self.0.path()
    }

    #[getter]
    fn path_segments<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyTuple>>> {
        self.0.path_segments().map(|v| PyTuple::new(py, v.collect::<Vec<_>>())).transpose()
    }

    #[getter]
    fn query_string(&self) -> Option<&str> {
        self.0.query()
    }

    #[getter]
    fn query(&self) -> MultiDictProxy {
        MultiDictProxy(self.0.query_pairs().collect())
    }

    #[getter]
    fn fragment(&self) -> Option<&str> {
        self.0.fragment()
    }

    fn with_fragment(&self, fragment: Option<&str>) -> Self {
        let mut url = self.0.clone();
        url.set_fragment(fragment);
        Url(url)
    }

    fn with_query(&self, query: Option<EncodablePairs>) -> PyResult<Self> {
        let mut url = self.0.clone();
        url.set_query(None);
        Self::extend_query_inner(&mut url, query)?;
        Ok(Url(url))
    }

    fn extend_query(&self, query: Option<EncodablePairs>) -> PyResult<Self> {
        let mut url = self.0.clone();
        Self::extend_query_inner(&mut url, query)?;
        Ok(Url(url))
    }

    fn with_query_string(&self, query: Option<&str>) -> Self {
        let mut url = self.0.clone();
        url.set_query(query);
        Url(url)
    }

    fn with_path(&self, path: &str) -> Self {
        let mut url = self.0.clone();
        url.set_path(path);
        Url(url)
    }

    fn with_path_segments(&self, segments: Vec<String>) -> PyResult<Self> {
        let mut url = self.0.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| PyValueError::new_err("cannot be base"))?;
            path.clear();
            path.extend(segments);
        }
        Ok(Url(url))
    }

    fn with_port(&self, port: Option<u16>) -> PyResult<Self> {
        let mut url = self.0.clone();
        url.set_port(port).map_err(|_| PyValueError::new_err("cannot be base"))?;
        Ok(Url(url))
    }

    fn with_host(&self, host: Option<&str>) -> PyResult<Self> {
        let mut url = self.0.clone();
        url.set_host(host).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Url(url))
    }

    fn with_ip_host(&self, addr: &str) -> PyResult<Self> {
        let addr = IpAddr::from_str(addr).map_err(|e| PyValueError::new_err(e.to_string()))?;
        let mut url = self.0.clone();
        url.set_ip_host(addr).map_err(|_| PyValueError::new_err("cannot be base"))?;
        Ok(Url(url))
    }

    fn with_username(&self, username: &str) -> PyResult<Self> {
        let mut url = self.0.clone();
        url.set_username(username).map_err(|_| PyValueError::new_err("cannot be base"))?;
        Ok(Url(url))
    }

    fn with_password(&self, password: Option<&str>) -> PyResult<Self> {
        let mut url = self.0.clone();
        url.set_password(password).map_err(|_| PyValueError::new_err("cannot be base"))?;
        Ok(Url(url))
    }

    fn with_scheme(&self, scheme: &str) -> PyResult<Self> {
        let mut url = self.0.clone();
        url.set_scheme(scheme).map_err(|_| PyValueError::new_err("Invalid scheme"))?;
        Ok(Url(url))
    }

    fn copy(&self) -> Self {
        Url(self.0.clone())
    }

    fn __copy__(&self) -> Self {
        Url(self.0.clone())
    }

    fn __truediv__(&self, other: &str) -> PyResult<Self> {
        self.join(other)
    }

    fn __str__(&self) -> &str {
        self.0.as_str()
    }

    fn __repr__(slf: Bound<Self>) -> PyResult<String> {
        let url_repr = slf.call_method0(intern!(slf.py(), "__str__"))?.repr()?;
        Ok(format!("Url({})", url_repr.to_str()?))
    }

    fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.0.hash(&mut hasher);
        hasher.finish()
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp) -> bool {
        match op {
            CompareOp::Lt => self.0 < other.0,
            CompareOp::Le => self.0 <= other.0,
            CompareOp::Eq => self.0 == other.0,
            CompareOp::Ne => self.0 != other.0,
            CompareOp::Gt => self.0 > other.0,
            CompareOp::Ge => self.0 >= other.0,
        }
    }
}
impl Url {
    fn extend_query_inner(url: &mut url::Url, query: Option<EncodablePairs>) -> PyResult<()> {
        if let Some(query) = query.map(|q| q.0) {
            let mut url_query = url.query_pairs_mut();
            let serializer = serde_urlencoded::Serializer::new(&mut url_query);
            query.serialize(serializer).map_err(|e| PyValueError::new_err(e.to_string()))?;
        }
        Ok(())
    }
}
impl From<reqwest::Url> for Url {
    fn from(value: reqwest::Url) -> Self {
        Url(value)
    }
}
impl From<UrlType> for Url {
    fn from(value: UrlType) -> Self {
        Url(value.0)
    }
}

#[derive(Clone)]
pub struct UrlType(pub url::Url);
impl<'py> FromPyObject<'py> for UrlType {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        if let Ok(url) = ob.downcast_exact::<Url>() {
            Ok(UrlType(url.borrow().0.clone()))
        } else {
            let url =
                url::Url::parse(ob.str()?.extract::<&str>()?).map_err(|e| PyValueError::new_err(e.to_string()))?;
            Ok(UrlType(url))
        }
    }
}
