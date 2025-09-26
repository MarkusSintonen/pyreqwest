use crate::http::header_map::header_map::HeaderMap;
use crate::http::header_map::iters::{HeaderMapItemsIter, HeaderMapKeysIter, HeaderMapValuesIter};
use crate::http::internal::types::{HeaderName, HeaderValue};
use pyo3::basic::CompareOp;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyIterator, PyList, PySet, PyString};

#[pyclass(frozen)]
pub struct HeaderMapItemsView(HeaderMap);
#[pymethods]
impl HeaderMapItemsView {
    fn __iter__(&self) -> PyResult<HeaderMapItemsIter> {
        HeaderMapItemsIter::new(self.0.clone_arc())
    }

    fn __len__(&self) -> PyResult<usize> {
        self.0.__len__()
    }

    fn __contains__(&self, kv: (String, String)) -> PyResult<bool> {
        let (key, val) = kv;
        self.0.ref_map(|map| {
            for v in map.get_all(key) {
                if v.as_bytes() == val.as_bytes() {
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }

    fn __reversed__(&self, py: Python) -> PyResult<Py<PyIterator>> {
        vec_rev_iter(py, self.to_vec()?)
    }

    fn __richcmp__(&self, other: Bound<PyAny>, op: CompareOp) -> PyResult<bool> {
        Ok(richcmp(self.to_vec()?, other, op))
    }

    fn __str__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        PyList::new(py, self.to_vec_sensitive()?)?.str()
    }

    fn __repr__(&self, py: Python) -> PyResult<String> {
        let repr = PyList::new(py, self.to_vec_sensitive()?)?.repr()?;
        Ok(format!("HeaderMapItemsView({})", repr.to_str()?))
    }

    // ItemsView AbstractSet methods

    fn __and__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__and__"), other)
    }

    fn __rand__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__rand__"), other)
    }

    fn __or__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__or__"), other)
    }

    fn __ror__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__ror__"), other)
    }

    fn __sub__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__sub__"), other)
    }

    fn __rsub__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__rsub__"), other)
    }

    fn __xor__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__xor__"), other)
    }

    fn __rxor__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__rxor__"), other)
    }

    fn isdisjoint<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "isdisjoint"), other)
    }
}
impl HeaderMapItemsView {
    pub fn new(map: HeaderMap) -> Self {
        HeaderMapItemsView(map)
    }

    fn to_vec(&self) -> PyResult<Vec<(HeaderName, HeaderValue)>> {
        self.0.ref_map(|map| {
            Ok(map
                .iter()
                .map(|(k, v)| (HeaderName(k.clone()), HeaderValue(v.clone())))
                .collect())
        })
    }

    fn to_vec_sensitive(&self) -> PyResult<Vec<(HeaderName, HeaderValue)>> {
        self.0.ref_map(|map| {
            let iter = map.iter().map(|(k, v)| {
                let v = if v.is_sensitive() {
                    HeaderValue::try_from("Sensitive").unwrap()
                } else {
                    HeaderValue(v.clone())
                };
                (HeaderName(k.clone()), v)
            });
            Ok(iter.collect())
        })
    }
}

#[pyclass(frozen)]
pub struct HeaderMapKeysView(HeaderMap);
#[pymethods]
impl HeaderMapKeysView {
    fn __iter__(&self) -> PyResult<HeaderMapKeysIter> {
        HeaderMapKeysIter::new(&self.0)
    }

    fn __len__(&self) -> PyResult<usize> {
        self.0.__len__()
    }

    fn __contains__(&self, key: &str) -> PyResult<bool> {
        self.0.__contains__(key)
    }

    fn __reversed__(&self, py: Python) -> PyResult<Py<PyIterator>> {
        vec_rev_iter(py, self.to_vec()?)
    }

    fn __richcmp__(&self, other: Bound<PyAny>, op: CompareOp) -> PyResult<bool> {
        Ok(richcmp(self.to_vec()?, other, op))
    }

    fn __str__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        PyList::new(py, self.to_vec()?)?.str()
    }

    fn __repr__(&self, py: Python) -> PyResult<String> {
        let repr = PyList::new(py, self.to_vec()?)?.repr()?;
        Ok(format!("HeaderMapKeysView({})", repr.to_str()?))
    }

    // KeysView AbstractSet methods

    fn __and__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__and__"), other)
    }

    fn __rand__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__rand__"), other)
    }

    fn __or__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__or__"), other)
    }

    fn __ror__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__ror__"), other)
    }

    fn __sub__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__sub__"), other)
    }

    fn __rsub__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__rsub__"), other)
    }

    fn __xor__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__xor__"), other)
    }

    fn __rxor__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "__rxor__"), other)
    }

    fn isdisjoint<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        set_op(self.to_vec()?, intern!(py, "isdisjoint"), other)
    }
}
impl HeaderMapKeysView {
    pub fn new(map: HeaderMap) -> Self {
        HeaderMapKeysView(map)
    }

    fn to_vec(&self) -> PyResult<Vec<HeaderName>> {
        self.0
            .ref_map(|map| Ok(map.iter().map(|(k, _)| HeaderName(k.clone())).collect()))
    }
}

#[pyclass(frozen)]
pub struct HeaderMapValuesView(HeaderMap);
#[pymethods]
impl HeaderMapValuesView {
    fn __iter__(&self) -> PyResult<HeaderMapValuesIter> {
        HeaderMapValuesIter::new(self.0.clone_arc())
    }

    fn __len__(&self) -> PyResult<usize> {
        self.0.__len__()
    }

    fn __contains__(&self, val: &str) -> PyResult<bool> {
        self.0.ref_map(|map| {
            for v in map.values() {
                if v.as_bytes() == val.as_bytes() {
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }

    fn __reversed__(&self, py: Python) -> PyResult<Py<PyIterator>> {
        vec_rev_iter(py, self.to_vec()?)
    }

    fn __str__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        PyList::new(py, self.to_vec_sensitive()?)?.str()
    }

    fn __repr__(&self, py: Python) -> PyResult<String> {
        let repr = PyList::new(py, self.to_vec_sensitive()?)?.repr()?;
        Ok(format!("HeaderMapValuesView({})", repr.to_str()?))
    }
}
impl HeaderMapValuesView {
    pub fn new(map: HeaderMap) -> Self {
        HeaderMapValuesView(map)
    }

    fn to_vec(&self) -> PyResult<Vec<HeaderValue>> {
        self.0
            .ref_map(|map| Ok(map.values().map(|v| HeaderValue(v.clone())).collect()))
    }

    fn to_vec_sensitive(&self) -> PyResult<Vec<HeaderValue>> {
        self.0.ref_map(|map| {
            let iter = map.iter().map(|(_, v)| {
                if v.is_sensitive() {
                    HeaderValue::try_from("Sensitive").unwrap()
                } else {
                    HeaderValue(v.clone())
                }
            });
            Ok(iter.collect())
        })
    }
}

fn vec_rev_iter<'py, T: IntoPyObject<'py>>(py: Python<'py>, mut v: Vec<T>) -> PyResult<Py<PyIterator>> {
    v.reverse();
    Ok(PyList::new(py, v)?.into_any().try_iter()?.unbind())
}

fn richcmp<'py, T: FromPyObject<'py> + Ord>(mut v: Vec<T>, other: Bound<'py, PyAny>, op: CompareOp) -> bool {
    let Ok(mut v2) = convert_vec::<T>(other) else {
        return matches!(op, CompareOp::Ne);
    };
    // HeaderMap is not guaranteed to be sorted by any criteria, so we sort it for comparison for set-like behavior
    v.sort();
    v2.sort();
    match op {
        CompareOp::Lt => v < v2,
        CompareOp::Le => v <= v2,
        CompareOp::Eq => v == v2,
        CompareOp::Ne => v != v2,
        CompareOp::Gt => v > v2,
        CompareOp::Ge => v >= v2,
    }
}

fn set_op<'py, T: IntoPyObject<'py>>(
    v: Vec<T>,
    op: &Bound<'py, PyString>,
    other: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let other_set = PySet::new(other.py(), other.try_iter()?.collect::<PyResult<Vec<_>>>()?)?;
    PySet::new(op.py(), v)?.call_method1(op, (other_set,))
}

fn convert_vec<'py, T: FromPyObject<'py> + Ord>(ob: Bound<'py, PyAny>) -> PyResult<Vec<T>> {
    ob.try_iter()?.map(|v| v?.extract::<T>()).collect()
}
