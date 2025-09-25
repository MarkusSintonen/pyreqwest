use crate::http::cookie::Cookie;
use crate::http::cookie::cookie::CookieType;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use time::OffsetDateTime;

#[pyclass]
pub struct CookieBuilder(Option<cookie::CookieBuilder<'static>>);

#[pymethods]
impl CookieBuilder {
    #[new]
    fn new(name: String, value: String) -> Self {
        Self(Some(cookie::CookieBuilder::new(name, value)))
    }

    #[staticmethod]
    fn from_cookie(cookie: CookieType) -> Self {
        Self(Some(cookie::Cookie::build(cookie.0)))
    }

    fn build(&mut self) -> PyResult<Cookie> {
        let inner = self
            .0
            .take()
            .ok_or_else(|| PyValueError::new_err("Cookie was already built"))?;
        Ok(Cookie(inner.build().into_owned()))
    }

    fn expires(slf: PyRefMut<'_, Self>, expires: Option<OffsetDateTime>) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |b| Ok(b.expires(expires)))
    }

    fn max_age(slf: PyRefMut<'_, Self>, max_age: time::Duration) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |b| Ok(b.max_age(max_age)))
    }

    fn domain(slf: PyRefMut<'_, Self>, domain: String) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |b| Ok(b.domain(domain)))
    }

    fn path(slf: PyRefMut<'_, Self>, path: String) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |b| Ok(b.path(path)))
    }

    fn secure(slf: PyRefMut<'_, Self>, secure: bool) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |b| Ok(b.secure(secure)))
    }

    fn http_only(slf: PyRefMut<'_, Self>, http_only: bool) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |b| Ok(b.http_only(http_only)))
    }

    fn same_site<'py>(slf: PyRefMut<'py, Self>, same_site: &str) -> PyResult<PyRefMut<'py, Self>> {
        let same_site = match same_site {
            "Strict" => cookie::SameSite::Strict,
            "Lax" => cookie::SameSite::Lax,
            "None" => cookie::SameSite::None,
            _ => return Err(PyValueError::new_err("invalid SameSite, expected 'Strict', 'Lax', or 'None'")),
        };
        Self::apply(slf, |b| Ok(b.same_site(same_site)))
    }

    fn partitioned(slf: PyRefMut<'_, Self>, partitioned: bool) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |b| Ok(b.partitioned(partitioned)))
    }

    fn permanent(slf: PyRefMut<'_, Self>) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |b| Ok(b.permanent()))
    }

    fn removal(slf: PyRefMut<'_, Self>) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |b| Ok(b.removal()))
    }
}
impl CookieBuilder {
    fn apply<F>(mut slf: PyRefMut<Self>, fun: F) -> PyResult<PyRefMut<Self>>
    where
        F: FnOnce(cookie::CookieBuilder<'static>) -> PyResult<cookie::CookieBuilder<'static>>,
        F: Send,
    {
        let builder = slf
            .0
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Cookie was already built"))?;
        slf.0 = Some(slf.py().detach(|| fun(builder))?);
        Ok(slf)
    }
}
