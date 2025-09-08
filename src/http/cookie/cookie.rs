use pyo3::basic::CompareOp;
use pyo3::exceptions::PyValueError;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyDict, PyIterator, PyString, PyTuple};
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
    fn http_only(&self) -> Option<bool> {
        self.0.http_only()
    }

    #[getter]
    fn secure(&self) -> Option<bool> {
        self.0.secure()
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
    fn partitioned(&self) -> Option<bool> {
        self.0.partitioned()
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
        let Ok(other) = other.extract::<CookieType>() else {
            return self.__str__(other.py()).rich_compare(other, CompareOp::Eq)?.extract();
        };
        Ok(self.0 == other.0)
    }

    fn __ne__(&self, other: Bound<PyAny>) -> PyResult<bool> {
        let Ok(other) = other.extract::<CookieType>() else {
            return self.__str__(other.py()).rich_compare(other, CompareOp::Ne)?.extract();
        };
        Ok(self.0 != other.0)
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
impl From<cookie::Cookie<'_>> for Cookie {
    fn from(cookie: cookie::Cookie<'_>) -> Self {
        Cookie(cookie.into_owned())
    }
}
impl From<&cookie::Cookie<'_>> for Cookie {
    fn from(cookie: &cookie::Cookie<'_>) -> Self {
        Cookie(cookie.clone().into_owned())
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
