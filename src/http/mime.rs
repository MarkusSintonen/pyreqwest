use pyo3::basic::CompareOp;
use pyo3::exceptions::PyValueError;
use pyo3::intern;
use pyo3::prelude::*;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::str::FromStr;

#[pyclass(frozen)]
pub struct Mime(mime::Mime);
#[pymethods]
impl Mime {
    #[staticmethod]
    fn parse(mime: String) -> PyResult<Self> {
        let inner: mime::Mime = mime
            .parse()
            .map_err(|e: mime::FromStrError| PyValueError::new_err(e.to_string()))?;
        Ok(Mime(inner))
    }

    #[getter]
    fn type_(&self) -> &str {
        self.0.type_().as_str()
    }

    #[getter]
    fn subtype(&self) -> &str {
        self.0.subtype().as_str()
    }

    #[getter]
    fn suffix(&self) -> Option<&str> {
        self.0.suffix().map(|v| v.as_str())
    }

    #[getter]
    fn parameters(&self) -> Vec<(String, String)> {
        self.0.params().map(|(n, v)| (n.to_string(), v.to_string())).collect()
    }

    #[getter]
    fn essence_str(&self) -> &str {
        self.0.essence_str()
    }

    pub fn get_param(&self, name: &str) -> Option<&str> {
        self.0.get_param(name).map(|v| v.as_str())
    }

    fn __copy__(&self) -> Self {
        Mime(self.0.clone())
    }

    fn __str__(&self) -> String {
        self.0.to_string()
    }

    fn __repr__(slf: Bound<Self>) -> PyResult<String> {
        let mime_repr = slf.call_method0(intern!(slf.py(), "__str__"))?.repr()?;
        Ok(format!("Mime({})", mime_repr.to_str()?))
    }

    fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.0.hash(&mut hasher);
        hasher.finish()
    }

    fn __richcmp__(&self, other: Bound<PyAny>, op: CompareOp) -> bool {
        fn cmp(this: &mime::Mime, other: &mime::Mime, op: CompareOp) -> bool {
            match op {
                CompareOp::Lt => this < other,
                CompareOp::Le => this <= other,
                CompareOp::Eq => this == other,
                CompareOp::Ne => this != other,
                CompareOp::Gt => this > other,
                CompareOp::Ge => this >= other,
            }
        }

        if let Ok(other) = other.downcast_exact::<Mime>() {
            cmp(&self.0, &other.get().0, op)
        } else if let Ok(other) = other.extract::<&str>() {
            mime::Mime::from_str(other).map_or(false, |m| cmp(&self.0, &m, op))
        } else {
            false
        }
    }
}
impl Mime {
    pub fn new(inner: mime::Mime) -> Self {
        Mime(inner)
    }
}
