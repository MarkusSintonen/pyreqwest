use pyo3::basic::CompareOp;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::borrow::Cow;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::IpAddr;
use std::str::FromStr;

#[derive(Clone)]
#[pyclass]
pub struct Url(pub url::Url);

#[pymethods]
impl Url {
    #[new]
    fn new(url: UrlType) -> Self {
        Url::parse(url)
    }

    #[staticmethod]
    pub fn parse(url: UrlType) -> Self {
        Url(url.0)
    }

    #[staticmethod]
    pub fn parse_with_params(mut url: UrlType, params: Vec<(String, String)>) -> PyResult<Self> {
        url.0.query_pairs_mut().extend_pairs(params);
        Ok(Url(url.0))
    }

    pub fn join(&self, input: &str) -> PyResult<Self> {
        self.0
            .join(input)
            .map(Url)
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    pub fn make_relative(&self, base: &Self) -> Option<String> {
        self.0.make_relative(&base.0)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn origin_ascii(&self) -> String {
        self.0.origin().ascii_serialization()
    }

    pub fn origin_unicode(&self) -> String {
        self.0.origin().unicode_serialization()
    }

    pub fn scheme(&self) -> &str {
        self.0.scheme()
    }

    pub fn is_special(&self) -> bool {
        self.0.is_special()
    }

    pub fn has_authority(&self) -> bool {
        self.0.has_authority()
    }

    pub fn authority(&self) -> &str {
        self.0.authority()
    }

    pub fn cannot_be_a_base(&self) -> bool {
        self.0.cannot_be_a_base()
    }

    pub fn username(&self) -> &str {
        self.0.username()
    }

    pub fn password(&self) -> Option<&str> {
        self.0.password()
    }

    pub fn has_host(&self) -> bool {
        self.0.has_host()
    }

    pub fn host_str(&self) -> Option<&str> {
        self.0.host_str()
    }

    pub fn domain(&self) -> Option<&str> {
        self.0.domain()
    }

    pub fn port(&self) -> Option<u16> {
        self.0.port()
    }

    pub fn port_or_known_default(&self) -> Option<u16> {
        self.0.port_or_known_default()
    }

    pub fn path(&self) -> &str {
        self.0.path()
    }

    pub fn path_segments(&self) -> Option<Vec<&str>> {
        self.0.path_segments().map(|v| v.collect())
    }

    pub fn query(&self) -> Option<&str> {
        self.0.query()
    }

    pub fn query_pairs(&self) -> Vec<(Cow<'_, str>, Cow<'_, str>)> {
        self.0.query_pairs().collect()
    }

    pub fn fragment(&self) -> Option<&str> {
        self.0.fragment()
    }

    pub fn set_fragment(&mut self, fragment: Option<&str>) {
        self.0.set_fragment(fragment);
    }

    pub fn set_query(&mut self, query: Option<&str>) {
        self.0.set_query(query);
    }

    pub fn set_query_pairs(&mut self, query: Vec<(String, String)>) {
        self.0.query_pairs_mut().clear();
        for (key, value) in query.iter() {
            self.0.query_pairs_mut().append_pair(key, value);
        }
    }

    pub fn set_path(&mut self, path: &str) {
        self.0.set_path(path);
    }

    pub fn set_path_segments(&mut self, segments: Vec<String>) -> PyResult<()> {
        let mut path = self
            .0
            .path_segments_mut()
            .map_err(|_| PyValueError::new_err("cannot be base"))?;
        path.clear();
        path.extend(segments);
        Ok(())
    }

    pub fn set_port(&mut self, port: Option<u16>) -> PyResult<()> {
        self.0
            .set_port(port)
            .map_err(|_| PyValueError::new_err("cannot be base"))
    }

    pub fn set_host(&mut self, host: Option<&str>) -> PyResult<()> {
        self.0.set_host(host).map_err(|e| PyValueError::new_err(e.to_string()))
    }

    pub fn set_ip_host(&mut self, addr: &str) -> PyResult<()> {
        let addr = IpAddr::from_str(addr).map_err(|e| PyValueError::new_err(e.to_string()))?;
        self.0
            .set_ip_host(addr)
            .map_err(|_| PyValueError::new_err("cannot be base"))
    }

    pub fn set_password(&mut self, password: Option<&str>) -> PyResult<()> {
        self.0
            .set_password(password)
            .map_err(|_| PyValueError::new_err("cannot be base"))
    }

    pub fn set_username(&mut self, username: &str) -> PyResult<()> {
        self.0
            .set_username(username)
            .map_err(|_| PyValueError::new_err("cannot be base"))
    }

    pub fn set_scheme(&mut self, scheme: &str) -> PyResult<()> {
        self.0
            .set_scheme(scheme)
            .map_err(|_| PyValueError::new_err("Invalid scheme"))
    }

    fn __repr__(slf: &Bound<'_, Self>) -> PyResult<String> {
        Ok(format!("Url({})", slf.borrow().0))
    }

    fn __str__(&self) -> String {
        self.0.to_string()
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
