use crate::http::header_map::iters::HeaderMapKeysIter;
use crate::http::header_map::views::{HeaderMapItemsView, HeaderMapKeysView, HeaderMapValuesView};
use crate::http::{HeaderName, HeaderValue};
use http::header::Entry;
use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyKeyError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyEllipsis, PyList, PyMapping, PySequence, PyString};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[pyclass]
pub struct HeaderMap(Arc<Mutex<Inner>>);
pub struct Inner {
    map: Option<http::HeaderMap>,
}
#[pymethods]
impl HeaderMap {
    #[new]
    #[pyo3(signature = (other=None))]
    fn py_new(other: Option<UpdateArg>) -> PyResult<Self> {
        let mut inner = http::HeaderMap::new();
        if let Some(other) = other {
            HeaderMap::extend_inner(&mut inner, other)?;
        }
        Ok(HeaderMap::from(inner))
    }

    // MutableMapping methods

    fn __getitem__(&self, key: &str) -> PyResult<HeaderValue> {
        self.ref_map(|map| match map.get(key) {
            Some(v) => Ok(HeaderValue(v.clone())),
            None => Err(PyKeyError::new_err(key.to_string())),
        })
    }

    fn __setitem__(&mut self, key: HeaderName, value: HeaderValue) -> PyResult<()> {
        self.mut_map(|map| {
            map.try_insert(key.0, value.0)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
                .map(|_| ())
        })
    }

    fn __delitem__(&mut self, key: &str) -> PyResult<()> {
        self.mut_map(|map| match map.try_entry(key) {
            Ok(Entry::Occupied(entry)) => {
                entry.remove_entry_mult();
                Ok(())
            }
            Ok(Entry::Vacant(_)) => Err(PyKeyError::new_err(key.to_string())),
            Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
        })
    }

    fn __iter__(&self) -> PyResult<HeaderMapKeysIter> {
        HeaderMapKeysIter::new(&self)
    }

    fn __bool__(&self) -> PyResult<bool> {
        self.ref_map(|map| Ok(!map.is_empty()))
    }

    fn __len__(&self) -> PyResult<usize> {
        self.total_len()
    }

    pub fn __contains__(&self, key: &str) -> PyResult<bool> {
        self.ref_map(|map| Ok(map.contains_key(key)))
    }

    fn items(&self) -> HeaderMapItemsView {
        HeaderMapItemsView::new(self.clone_arc())
    }

    fn keys(&self) -> HeaderMapKeysView {
        HeaderMapKeysView::new(self.clone_arc())
    }

    fn values(&self) -> HeaderMapValuesView {
        HeaderMapValuesView::new(self.clone_arc())
    }

    #[pyo3(signature = (key, default=None))]
    fn get<'py>(&self, py: Python<'py>, key: &str, default: Option<&str>) -> PyResult<Bound<'py, PyAny>> {
        self.ref_map(|map| match map.get(key) {
            Some(v) => HeaderValue(v.clone()).into_bound_py_any(py),
            None => default.into_bound_py_any(py),
        })
    }

    fn __eq__(&self, other: Bound<PyAny>) -> PyResult<bool> {
        self.ref_map(|map| {
            if let Ok(other_map) = other.downcast_exact::<HeaderMap>() {
                return other_map.try_borrow()?.ref_map(|other| Ok(map == other));
            } else if let Ok(other_dict) = other.downcast_into::<PyMapping>() {
                if other_dict.len()? != map.len() {
                    return Ok(false);
                }
                let mut other_map = http::HeaderMap::new();
                return match HeaderMap::extend_inner(&mut other_map, UpdateArg::Mapping(other_dict)) {
                    Ok(()) => Ok(map == &other_map),
                    Err(_) => Ok(false),
                };
            }
            Ok(false)
        })
    }

    fn __ne__(&self, other: Bound<PyAny>) -> PyResult<bool> {
        Ok(!self.__eq__(other)?)
    }

    #[pyo3(signature = (key, default=PopArg::NotPresent(ellipsis())))]
    fn pop<'py>(&mut self, py: Python<'py>, key: &str, default: PopArg<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.mut_map(|map| match map.remove(key) {
            Some(v) => HeaderValue(v).into_bound_py_any(py),
            None => match default {
                PopArg::Value(v) => Ok(v),
                PopArg::NotPresent(_) => Err(PyKeyError::new_err(key.to_string())),
            },
        })
    }

    fn popitem(&mut self) -> PyResult<(HeaderName, HeaderValue)> {
        self.mut_map(|map| {
            let k = match map.iter().next() {
                Some((k, _)) => k.clone(),
                None => return Err(PyKeyError::new_err("popitem(): HeaderMap is empty")),
            };
            match map.remove(&k) {
                Some(v) => Ok((HeaderName(k), HeaderValue(v))),
                None => Err(PyKeyError::new_err(k.to_string())),
            }
        })
    }

    fn clear(&mut self) -> PyResult<()> {
        self.mut_map(|map| Ok(map.clear()))
    }

    fn update(&mut self, other: UpdateArg) -> PyResult<()> {
        fn insert(map: &mut http::HeaderMap, tup: Bound<PyAny>) -> PyResult<()> {
            let (k, v): (HeaderName, HeaderValue) = tup.extract()?;
            map.try_insert(k.0, v.0)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
                .map(|_| ())
        }

        self.mut_map(|map| match other {
            UpdateArg::Mapping(mapping) => mapping.items()?.iter().try_for_each(|tup| insert(map, tup)),
            UpdateArg::Sequence(seq) => seq.try_iter()?.try_for_each(|tup| insert(map, tup?)),
        })
    }

    fn setdefault(&mut self, key: HeaderName, default: HeaderValue) -> PyResult<HeaderValue> {
        self.mut_map(|map| match map.try_entry(key.0) {
            Ok(Entry::Occupied(entry)) => Ok(HeaderValue(entry.get().clone())),
            Ok(Entry::Vacant(entry)) => {
                entry
                    .try_insert(default.0.clone())
                    .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
                Ok(default)
            }
            Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
        })
    }

    // Inner HeaderMap methods

    pub fn total_len(&self) -> PyResult<usize> {
        self.ref_map(|map| Ok(map.len()))
    }

    fn keys_len(&self) -> PyResult<usize> {
        self.ref_map(|map| Ok(map.keys_len()))
    }

    pub fn get_one<'py>(&self, key: &str) -> PyResult<Option<HeaderValue>> {
        self.ref_map(|map| Ok(map.get(key).map(|v| HeaderValue(v.clone()))))
    }

    fn get_all<'py>(&self, py: Python<'py>, key: &str) -> PyResult<Bound<'py, PyList>> {
        PyList::new(py, self.get_all_vec(key)?)
    }

    fn insert(&mut self, key: HeaderName, value: HeaderValue) -> PyResult<Option<HeaderValue>> {
        self.mut_map(|map| match map.try_insert(key.0, value.0) {
            Ok(Some(v)) => Ok(Some(HeaderValue(v))),
            Ok(None) => Ok(None),
            Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
        })
    }

    fn append(&mut self, key: HeaderName, value: HeaderValue) -> PyResult<bool> {
        self.mut_map(|map| {
            map.try_append(key.0, value.0)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
        })
    }

    fn remove(&mut self, key: &str) -> PyResult<Option<HeaderValue>> {
        self.mut_map(|map| match map.remove(key) {
            Some(v) => Ok(Some(HeaderValue(v))),
            None => Ok(None),
        })
    }

    #[pyo3(signature = (key, default=PopArg::NotPresent(ellipsis())))]
    fn pop_all<'py>(&mut self, py: Python<'py>, key: &str, default: PopArg<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.mut_map(|map| match map.try_entry(key) {
            Ok(Entry::Occupied(entry)) => {
                let vals = entry.remove_entry_mult().1.map(|v| HeaderValue(v)).collect::<Vec<_>>();
                Ok(PyList::new(py, vals)?.into_any())
            }
            Ok(Entry::Vacant(_)) => match default {
                PopArg::Value(v) => Ok(v),
                PopArg::NotPresent(_) => Err(PyKeyError::new_err(key.to_string())),
            },
            Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
        })
    }

    // Additional methods

    fn extend(&mut self, other: UpdateArg) -> PyResult<()> {
        self.mut_map(|map| HeaderMap::extend_inner(map, other))
    }

    fn dict_multi_value<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        fn convert<'py>(py: Python<'py>, map: &http::HeaderMap) -> PyResult<Bound<'py, PyDict>> {
            let dict = PyDict::new(py);
            for (key, value) in map.iter() {
                let key = key.as_str();
                let value = value.to_str().map_err(|e| PyValueError::new_err(e.to_string()))?;
                match dict.get_item(key)? {
                    None => dict.set_item(key, PyList::new(py, vec![value])?)?,
                    Some(existing) => existing.downcast_exact::<PyList>()?.append(value)?,
                }
            }
            Ok(dict)
        }
        self.ref_map(|map| convert(py, map))
    }

    fn copy(&self) -> PyResult<Self> {
        self.__copy__()
    }

    fn __copy__(&self) -> PyResult<Self> {
        self.ref_map(|map| Ok(HeaderMap::from(map.clone())))
    }

    fn __str__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        self.dict_multi_value(py)?.str()
    }

    fn __repr__(&self, py: Python) -> PyResult<String> {
        Ok(format!("HeaderMap({})", self.__str__(py)?.repr()?.to_str()?))
    }
}
impl HeaderMap {
    pub fn new() -> Self {
        let inner = Inner {
            map: Some(http::HeaderMap::new()),
        };
        HeaderMap(Arc::new(Mutex::new(inner)))
    }

    pub fn try_clone(&self) -> PyResult<Self> {
        self.ref_map(|map| Ok(HeaderMap::from(map.clone())))
    }

    pub fn try_clone_inner(&self) -> PyResult<http::HeaderMap> {
        self.ref_map(|map| Ok(map.clone()))
    }

    pub fn try_take_inner(&mut self) -> PyResult<http::HeaderMap> {
        let mut inner = self.0.lock().unwrap();
        inner
            .map
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("HeaderMap was already consumed"))
    }

    pub fn clone_arc(&self) -> Self {
        HeaderMap(Arc::clone(&self.0))
    }

    pub fn ref_map<U, F: FnOnce(&http::HeaderMap) -> PyResult<U>>(&self, f: F) -> PyResult<U> {
        match self.0.lock().unwrap().map.as_ref() {
            Some(map) => f(map),
            None => Err(PyRuntimeError::new_err("HeaderMap was already consumed")),
        }
    }

    pub fn mut_map<U, F: FnOnce(&mut http::HeaderMap) -> PyResult<U>>(&self, f: F) -> PyResult<U> {
        let mut inner = self.0.lock().unwrap();
        match inner.map.as_mut() {
            Some(map) => f(map),
            None => Err(PyRuntimeError::new_err("HeaderMap was already consumed")),
        }
    }

    pub fn keys_once_deque(&self) -> PyResult<VecDeque<HeaderName>> {
        self.ref_map(|map| {
            Ok(map
                .keys()
                .into_iter()
                .map(|k| HeaderName(k.clone()))
                .collect::<VecDeque<_>>())
        })
    }

    pub fn keys_mult_deque(&self) -> PyResult<VecDeque<HeaderName>> {
        self.ref_map(|map| {
            Ok(map
                .iter()
                .into_iter()
                .map(|(k, _)| HeaderName(k.clone()))
                .collect::<VecDeque<_>>())
        })
    }

    pub fn get_all_vec(&self, key: &str) -> PyResult<Vec<HeaderValue>> {
        self.ref_map(|map| {
            Ok(map
                .get_all(key)
                .into_iter()
                .map(|v| HeaderValue(v.clone()))
                .collect::<Vec<_>>())
        })
    }

    pub fn get_all_extend_to_deque(&self, key: &str, deque: &mut VecDeque<HeaderValue>) -> PyResult<()> {
        self.ref_map(|map| {
            deque.extend(map.get_all(key).into_iter().map(|v| HeaderValue(v.clone())));
            Ok(())
        })
    }

    fn extend_inner(map: &mut http::HeaderMap, other: UpdateArg) -> PyResult<()> {
        fn append(map: &mut http::HeaderMap, tup: Bound<PyAny>) -> PyResult<()> {
            let (k, v): (HeaderName, HeaderValue) = tup.extract()?;
            map.try_append(k.0, v.0)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
                .map(|_| ())
        }

        match other {
            UpdateArg::Mapping(mapping) => mapping.items()?.iter().try_for_each(|tup| append(map, tup)),
            UpdateArg::Sequence(seq) => seq.try_iter()?.try_for_each(|tup| append(map, tup?)),
        }
    }
}
impl From<http::HeaderMap> for HeaderMap {
    fn from(value: http::HeaderMap) -> Self {
        let inner = Inner { map: Some(value) };
        HeaderMap(Arc::new(Mutex::new(inner)))
    }
}

#[derive(FromPyObject)]
enum PopArg<'py> {
    #[allow(dead_code)]
    NotPresent(Py<PyEllipsis>),
    Value(Bound<'py, PyAny>),
}
fn ellipsis() -> Py<PyEllipsis> {
    Python::with_gil(|py| PyEllipsis::get(py).to_owned().unbind())
}

#[derive(FromPyObject)]
enum UpdateArg<'py> {
    Mapping(Bound<'py, PyMapping>),
    Sequence(Bound<'py, PySequence>),
}

pub struct HeaderArg(pub HeaderMap);
impl<'py> FromPyObject<'py> for HeaderArg {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        if let Ok(map) = ob.downcast_exact::<HeaderMap>() {
            Ok(HeaderArg(map.try_borrow()?.try_clone()?))
        } else {
            Ok(HeaderArg(HeaderMap::py_new(Some(ob.extract::<UpdateArg>()?))?))
        }
    }
}
