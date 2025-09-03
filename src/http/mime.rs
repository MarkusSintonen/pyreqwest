use pyo3::basic::CompareOp;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyDict, PyIterator, PyString, PyTuple};
use pyo3::{IntoPyObjectExt, intern};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::str::FromStr;

#[pyclass(frozen)]
pub struct Mime(mime::Mime);
#[pymethods]
impl Mime {
    #[staticmethod]
    fn parse(mime: &str) -> PyResult<Self> {
        Ok(Mime(Mime::parse_inner(mime)?))
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

    fn __str__<'py>(&self, py: Python<'py>) -> Bound<'py, PyString> {
        PyString::new(py, self.0.as_ref())
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

    fn __richcmp__<'py>(
        &self,
        py: Python<'py>,
        other: Bound<'py, PyAny>,
        op: CompareOp,
    ) -> PyResult<Bound<'py, PyAny>> {
        let Ok(other) = other.extract::<MimeType>() else {
            return self.__str__(py).rich_compare(other, op);
        };
        match op {
            CompareOp::Lt => self.0 < other.0,
            CompareOp::Le => self.0 <= other.0,
            CompareOp::Eq => self.0 == other.0,
            CompareOp::Ne => self.0 != other.0,
            CompareOp::Gt => self.0 > other.0,
            CompareOp::Ge => self.0 >= other.0,
        }
        .into_bound_py_any(py)
    }

    // Sequence methods

    fn __len__(&self) -> usize {
        self.0.as_ref().len()
    }

    fn __contains__(&self, item: &str) -> bool {
        self.0.as_ref().contains(item)
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
impl Mime {
    pub fn new(inner: mime::Mime) -> Self {
        Mime(inner)
    }

    fn parse_inner(mime: &str) -> PyResult<mime::Mime> {
        mime::Mime::from_str(mime).map_err(|e| PyValueError::new_err(e.to_string()))
    }
}

pub struct MimeType(pub mime::Mime);
impl<'py> FromPyObject<'py> for MimeType {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        if let Ok(mime) = ob.downcast_exact::<Mime>() {
            return Ok(MimeType(mime.get().0.clone()));
        }
        if let Ok(str) = ob.extract::<&str>() {
            return Ok(MimeType(Mime::parse_inner(str)?));
        }
        Ok(MimeType(Mime::parse_inner(ob.str()?.extract::<&str>()?)?))
    }
}
