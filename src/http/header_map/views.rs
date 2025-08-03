use crate::http::HeaderValue;
use crate::http::header_map::header_map::HeaderMap;
use crate::http::header_map::iters::{HeaderMapItemsIter, HeaderMapKeysIter, HeaderMapValuesIter};
use pyo3::basic::CompareOp;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyIterator, PyList, PySet};

#[pyclass]
pub struct HeaderMapItemsView {
    map: HeaderMap,
}
#[pymethods]
impl HeaderMapItemsView {
    fn __iter__(&self) -> PyResult<HeaderMapItemsIter> {
        HeaderMapItemsIter::new(self.map.clone_arc())
    }

    fn __len__(&self) -> PyResult<usize> {
        self.map.total_len()
    }

    fn __contains__(&self, kv: (String, String)) -> PyResult<bool> {
        let (key, val) = kv;
        self.map.ref_map(|map| match map.get(key) {
            Some(header_value) => HeaderValue::str_res(header_value).map(|v| v == val),
            None => Ok(false),
        })
    }

    fn __reversed__(&self, py: Python) -> PyResult<Py<PyIterator>> {
        let mut items = self.to_vec()?;
        items.reverse();
        Ok(PyList::new(py, items)?.into_any().try_iter()?.unbind())
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp) -> PyResult<bool> {
        let v1 = self.to_vec()?;
        let v2 = other.to_vec()?;
        let res = match op {
            CompareOp::Lt => v1 < v2,
            CompareOp::Le => v1 <= v2,
            CompareOp::Eq => v1 == v2,
            CompareOp::Ne => v1 != v2,
            CompareOp::Gt => v1 > v2,
            CompareOp::Ge => v1 >= v2,
        };
        Ok(res)
    }

    // ItemsView AbstractSet methods

    fn __and__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__and__"), (other,))
    }

    fn __rand__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__rand__"), (other,))
    }

    fn __or__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__or__"), (other,))
    }

    fn __ror__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__ror__"), (other,))
    }

    fn __sub__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__sub__"), (other,))
    }

    fn __rsub__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__rsub__"), (other,))
    }

    fn __xor__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__xor__"), (other,))
    }

    fn __rxor__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__rxor__"), (other,))
    }
}
impl HeaderMapItemsView {
    pub fn new(map: HeaderMap) -> PyResult<Self> {
        Ok(HeaderMapItemsView { map })
    }

    fn to_vec(&self) -> PyResult<Vec<(String, String)>> {
        self.map.ref_map(|map| {
            map.iter()
                .map(|(k, v)| Ok((k.as_str().to_string(), HeaderValue::str_res(v)?.to_string())))
                .collect::<Result<Vec<_>, _>>()
        })
    }

    fn py_set<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PySet>> {
        PySet::new(py, self.to_vec()?)
    }
}

#[pyclass]
pub struct HeaderMapKeysView(HeaderMapItemsView);
#[pymethods]
impl HeaderMapKeysView {
    fn __iter__(&self) -> PyResult<HeaderMapKeysIter> {
        HeaderMapKeysIter::new(self.0.map.clone_arc())
    }

    fn __len__(&self) -> PyResult<usize> {
        self.0.map.total_len()
    }

    fn __contains__(&self, key: &str) -> PyResult<bool> {
        self.0.map.__contains__(key)
    }

    fn __reversed__(&self, py: Python) -> PyResult<Py<PyIterator>> {
        let mut items = self.to_vec()?;
        items.reverse();
        Ok(PyList::new(py, items)?.into_any().try_iter()?.unbind())
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp) -> PyResult<bool> {
        let v1 = self.to_vec()?;
        let v2 = other.to_vec()?;
        let res = match op {
            CompareOp::Lt => v1 < v2,
            CompareOp::Le => v1 <= v2,
            CompareOp::Eq => v1 == v2,
            CompareOp::Ne => v1 != v2,
            CompareOp::Gt => v1 > v2,
            CompareOp::Ge => v1 >= v2,
        };
        Ok(res)
    }

    // KeysView AbstractSet methods

    fn __and__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__and__"), (other,))
    }

    fn __rand__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__rand__"), (other,))
    }

    fn __or__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__or__"), (other,))
    }

    fn __ror__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__ror__"), (other,))
    }

    fn __sub__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__sub__"), (other,))
    }

    fn __rsub__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__rsub__"), (other,))
    }

    fn __xor__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__xor__"), (other,))
    }

    fn __rxor__<'py>(&self, py: Python<'py>, other: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.py_set(py)?.call_method1(intern!(py, "__rxor__"), (other,))
    }
}
impl HeaderMapKeysView {
    pub fn new(map: HeaderMap) -> PyResult<Self> {
        Ok(HeaderMapKeysView(HeaderMapItemsView::new(map)?))
    }

    fn to_vec(&self) -> PyResult<Vec<String>> {
        self.0
            .map
            .ref_map(|map| Ok(map.keys().map(|k| k.as_str().to_string()).collect::<Vec<_>>()))
    }

    fn py_set<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PySet>> {
        PySet::new(py, self.to_vec()?)
    }
}

#[pyclass]
pub struct HeaderMapValuesView(HeaderMapItemsView);
#[pymethods]
impl HeaderMapValuesView {
    fn __iter__(&self) -> PyResult<HeaderMapValuesIter> {
        HeaderMapValuesIter::new(self.0.map.clone_arc())
    }

    fn __len__(&self) -> PyResult<usize> {
        self.0.map.total_len()
    }

    fn __contains__(&self, val: &str) -> PyResult<bool> {
        self.0.map.ref_map(|map| {
            for v in map.values() {
                if HeaderValue::str_res(v)? == val {
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }

    fn __reversed__(&self, py: Python) -> PyResult<Py<PyIterator>> {
        let mut items = self.to_vec()?;
        items.reverse();
        Ok(PyList::new(py, items)?.into_any().try_iter()?.unbind())
    }
}
impl HeaderMapValuesView {
    pub fn new(map: HeaderMap) -> PyResult<Self> {
        Ok(HeaderMapValuesView(HeaderMapItemsView::new(map)?))
    }

    fn to_vec(&self) -> PyResult<Vec<String>> {
        self.0
            .map
            .ref_map(|map| Ok(map.keys().map(|k| k.as_str().to_string()).collect::<Vec<_>>()))
    }
}
