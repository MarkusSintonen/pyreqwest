use pyo3::exceptions::PyValueError;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyDict, PyIterator, PyString, PyTuple};
use std::borrow::Cow;
use std::hash::{DefaultHasher, Hash, Hasher};
use time::{Duration, OffsetDateTime};

#[pyclass(frozen)]
pub struct Cookie(pub cookie::Cookie<'static>);

#[pymethods]
impl Cookie {
    #[new]
    fn new(name: String, value: String) -> Self {
        Self(cookie::Cookie::new(name, value).into_owned())
    }

    #[staticmethod]
    fn parse(cookie: &str) -> PyResult<Self> {
        Self::parse_inner(cookie).map(|cookie| Self(cookie.into_owned()))
    }

    #[staticmethod]
    fn parse_encoded(cookie: &str) -> PyResult<Self> {
        cookie::Cookie::parse_encoded(cookie)
            .map(|cookie| Self(cookie.into_owned()))
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn split_parse(cookie: &str) -> PyResult<Vec<Self>> {
        cookie::Cookie::split_parse(cookie)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PyValueError::new_err(e.to_string()))
            .map(|cookies| cookies.into_iter().map(|c| Self(c.into_owned())).collect())
    }

    #[staticmethod]
    fn split_parse_encoded(cookie: &str) -> PyResult<Vec<Self>> {
        cookie::Cookie::split_parse_encoded(cookie)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PyValueError::new_err(e.to_string()))
            .map(|cookies| cookies.into_iter().map(|c| Self(c.into_owned())).collect())
    }

    #[getter]
    fn name(&self) -> &str {
        self.0.name()
    }

    #[getter]
    fn value(&self) -> &str {
        self.0.value()
    }

    #[getter]
    fn value_trimmed(&self) -> &str {
        self.0.value_trimmed()
    }

    #[getter]
    fn http_only(&self) -> bool {
        self.0.http_only().unwrap_or_default()
    }

    #[getter]
    fn secure(&self) -> bool {
        self.0.secure().unwrap_or_default()
    }

    #[getter]
    fn same_site(&self) -> Option<&str> {
        match self.0.same_site() {
            Some(cookie::SameSite::Strict) => Some("Strict"),
            Some(cookie::SameSite::Lax) => Some("Lax"),
            Some(cookie::SameSite::None) => Some("None"),
            None => None,
        }
    }

    #[getter]
    fn partitioned(&self) -> bool {
        self.0.partitioned().unwrap_or_default()
    }

    #[getter]
    fn max_age(&self) -> Option<Duration> {
        self.0.max_age()
    }

    #[getter]
    fn path(&self) -> Option<&str> {
        self.0.path()
    }

    #[getter]
    fn domain(&self) -> Option<&str> {
        self.0.domain()
    }

    #[getter]
    fn expires_datetime(&self) -> Option<OffsetDateTime> {
        self.0.expires_datetime()
    }

    fn encode(&self) -> String {
        self.0.encoded().to_string()
    }

    fn stripped(&self) -> String {
        self.0.stripped().to_string()
    }

    fn with_name(&self, name: String) -> Self {
        let mut cookie = self.0.clone();
        cookie.set_name(name);
        Cookie(cookie)
    }

    fn with_value(&self, value: String) -> Self {
        let mut cookie = self.0.clone();
        cookie.set_value(value);
        Cookie(cookie)
    }

    fn with_http_only(&self, http_only: bool) -> Self {
        let mut cookie = self.0.clone();
        cookie.set_http_only(http_only);
        Cookie(cookie)
    }

    fn with_secure(&self, secure: bool) -> Self {
        let mut cookie = self.0.clone();
        cookie.set_secure(secure);
        Cookie(cookie)
    }

    fn with_same_site(&self, same_site: Option<&str>) -> PyResult<Self> {
        let same_site = match same_site {
            Some("Strict") => Some(cookie::SameSite::Strict),
            Some("Lax") => Some(cookie::SameSite::Lax),
            Some("None") => Some(cookie::SameSite::None),
            None => None,
            _ => return Err(PyValueError::new_err("invalid SameSite, expected 'Strict', 'Lax', 'None', or None")),
        };
        let mut cookie = self.0.clone();
        cookie.set_same_site(same_site);
        Ok(Cookie(cookie))
    }

    fn with_partitioned(&self, partitioned: bool) -> Self {
        let mut cookie = self.0.clone();
        cookie.set_partitioned(partitioned);
        Cookie(cookie)
    }

    fn with_max_age(&self, max_age: Option<Duration>) -> Self {
        let mut cookie = self.0.clone();
        cookie.set_max_age(max_age);
        Cookie(cookie)
    }

    fn with_path(&self, path: Option<String>) -> Self {
        let mut cookie = self.0.clone();
        match path {
            Some(val) => cookie.set_path(Cow::Owned(val)),
            None => cookie.unset_path(),
        }
        Cookie(cookie)
    }

    fn with_domain(&self, domain: Option<String>) -> Self {
        let mut cookie = self.0.clone();
        match domain {
            Some(val) => cookie.set_domain(Cow::Owned(val)),
            None => cookie.unset_domain(),
        }
        Cookie(cookie)
    }

    fn with_expires_datetime(&self, expires: Option<OffsetDateTime>) -> Self {
        let mut cookie = self.0.clone();
        cookie.set_expires(expires);
        Cookie(cookie)
    }

    fn __copy__(&self) -> Self {
        Cookie(self.0.clone())
    }

    fn __str__<'py>(&self, py: Python<'py>) -> Bound<'py, PyString> {
        PyString::new(py, &self.0.to_string())
    }

    fn __repr__(&self, py: Python) -> PyResult<String> {
        Ok(format!("Cookie({})", self.__str__(py).repr()?.to_str()?))
    }

    fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.0.to_string().hash(&mut hasher);
        hasher.finish()
    }

    fn __eq__(&self, other: Bound<PyAny>) -> PyResult<bool> {
        other.extract::<CookieType>().map_or(Ok(false), |c| Ok(c.0 == self.0))
    }

    fn __ne__(&self, other: Bound<PyAny>) -> PyResult<bool> {
        other.extract::<CookieType>().map_or(Ok(true), |c| Ok(c.0 != self.0))
    }

    // Sequence methods

    fn __len__(&self) -> usize {
        self.0.to_string().len()
    }

    fn __contains__(&self, item: &str) -> bool {
        self.0.to_string().contains(item)
    }

    fn __getitem__<'py>(&self, py: Python<'py>, k: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.__str__(py).get_item(k)
    }

    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.__str__(py).try_iter()
    }

    fn __reversed__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        static REVERSED: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
        REVERSED.import(py, "builtins", "reversed")?.call1((self.__str__(py),))
    }

    #[pyo3(signature = (*args, **kwargs))]
    fn index<'py>(
        &self,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.__str__(args.py())
            .call_method(intern!(args.py(), "index"), args, kwargs)
    }

    #[pyo3(signature = (*args, **kwargs))]
    fn count<'py>(
        &self,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.__str__(args.py())
            .call_method(intern!(args.py(), "count"), args, kwargs)
    }
}
impl Cookie {
    fn parse_inner(cookie: &str) -> PyResult<cookie::Cookie<'_>> {
        cookie::Cookie::parse(cookie).map_err(|e| PyValueError::new_err(e.to_string()))
    }
}
impl From<cookie_store::Cookie<'_>> for Cookie {
    fn from(cookie: cookie_store::Cookie<'_>) -> Self {
        Cookie(cookie::Cookie::from(cookie).into_owned())
    }
}
impl From<&cookie_store::Cookie<'_>> for Cookie {
    fn from(cookie: &cookie_store::Cookie<'_>) -> Self {
        Cookie(cookie::Cookie::from(cookie.clone()).into_owned())
    }
}

pub struct CookieType(pub cookie::Cookie<'static>);
impl<'py> FromPyObject<'py> for CookieType {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        if let Ok(cookie) = ob.downcast_exact::<Cookie>() {
            return Ok(CookieType(cookie.get().0.clone()));
        }
        if let Ok(str) = ob.extract::<&str>() {
            return Ok(CookieType(Cookie::parse_inner(str)?.into_owned()));
        }
        Ok(CookieType(Cookie::parse_inner(ob.str()?.to_str()?)?.into_owned()))
    }
}
