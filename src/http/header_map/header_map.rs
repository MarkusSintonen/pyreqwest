use crate::http::header_map::iters::HeaderMapKeysIter;
use crate::http::header_map::views::{HeaderMapItemsView, HeaderMapKeysView, HeaderMapValuesView};
use crate::internal::types::{HeaderName, HeaderValue};
use crate::internal::utils::{KeyValPairs, ellipsis};
use http::header::Entry;
use pyo3::exceptions::{PyKeyError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyEllipsis, PyList, PyString};
use pyo3::{IntoPyObjectExt, intern};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

type Inner = Option<http::HeaderMap>;

#[pyclass(frozen)]
pub struct HeaderMap(Arc<Mutex<Inner>>);
#[pymethods]
impl HeaderMap {
    #[new]
    #[pyo3(signature = (other=None))]
    fn new_py(other: Option<KeyValPairs>) -> PyResult<Self> {
        if let Some(other) = other {
            let mut inner = http::HeaderMap::with_capacity(other.len()?);
            HeaderMap::extend_inner(&mut inner, other)?;
            Ok(HeaderMap::from(inner))
        } else {
            Ok(HeaderMap::from(http::HeaderMap::new()))
        }
    }

    // MutableMapping methods

    fn __getitem__(&self, key: &str) -> PyResult<HeaderValue> {
        self.ref_map(|map| match map.get(key) {
            Some(v) => Ok(HeaderValue(v.clone())),
            None => Err(PyKeyError::new_err(key.to_string())),
        })
    }

    fn __setitem__(&self, key: HeaderName, value: HeaderValue) -> PyResult<()> {
        self.mut_map(|map| {
            map.try_insert(key.0, value.0)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
                .map(|_| ())
        })
    }

    fn __delitem__(&self, key: &str) -> PyResult<()> {
        let key = match http::HeaderName::try_from(key) {
            Ok(name) => name,
            Err(_) => return Err(PyKeyError::new_err(key.to_string())), // Invalid key, can not be present in map
        };

        self.mut_map(|map| match map.try_entry(key) {
            Ok(Entry::Occupied(entry)) => {
                entry.remove_entry_mult();
                Ok(())
            }
            Ok(Entry::Vacant(entry)) => Err(PyKeyError::new_err(entry.into_key().to_string())),
            Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
        })
    }

    fn __iter__(&self) -> PyResult<HeaderMapKeysIter> {
        HeaderMapKeysIter::new(self)
    }

    fn __bool__(&self) -> PyResult<bool> {
        self.ref_map(|map| Ok(!map.is_empty()))
    }

    pub fn __len__(&self) -> PyResult<usize> {
        self.ref_map(|map| Ok(map.len()))
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
    fn get<'py>(&self, py: Python<'py>, key: &str, default: Option<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
        self.ref_map(|map| match map.get(key) {
            Some(v) => HeaderValue(v.clone()).into_bound_py_any(py),
            None => default.into_bound_py_any(py),
        })
    }

    fn __eq__(&self, py: Python, other: Bound<PyAny>) -> PyResult<bool> {
        self.ref_map(|map| {
            if let Ok(other) = other.downcast_exact::<HeaderMap>() {
                other.get().ref_map(|other| Ok(map == other))
            } else if let Ok(other) = other.extract::<HeaderMap>() {
                other.ref_map(|other| Ok(map == other))
            } else {
                Self::dict_multi_value_inner(map, py, false)?.eq(other)
            }
        })
    }

    fn __ne__(&self, py: Python, other: Bound<PyAny>) -> PyResult<bool> {
        Ok(!self.__eq__(py, other)?)
    }

    #[pyo3(signature = (key, default=PopArg::NotPresent(ellipsis())))]
    fn pop<'py>(&self, py: Python<'py>, key: &str, default: PopArg<'py>) -> PyResult<Bound<'py, PyAny>> {
        let key = match http::HeaderName::try_from(key) {
            Ok(name) => name,
            Err(_) => return default.res(key.to_string()), // Invalid key, can not be present in map
        };
        self.mut_map(|map| Self::pop_inner(map, py, &key, default))
    }

    fn popitem<'py>(&self, py: Python<'py>) -> PyResult<(HeaderName, Bound<'py, PyAny>)> {
        self.mut_map(|map| {
            let key = match map.iter().next() {
                Some((key, _)) => Ok(key.clone()),
                None => Err(PyKeyError::new_err("popitem(): HeaderMap is empty")),
            }?;
            let val = Self::pop_inner(map, py, &key, PopArg::NotPresent(ellipsis()))?;
            Ok((HeaderName(key), val))
        })
    }

    fn clear(&self) -> PyResult<()> {
        self.mut_map(|map| {
            map.clear();
            Ok(())
        })
    }

    #[pyo3(signature = (other=None, **py_kwargs))]
    fn update(&self, other: Option<KeyValPairs>, py_kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        fn insert(map: &mut http::HeaderMap, k: HeaderName, v: HeaderValue) -> PyResult<()> {
            map.try_insert(k.0, v.0)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
                .map(|_| ())
        }

        self.mut_map(|map| {
            if let Some(other) = other {
                other.for_each(|(k, v)| insert(map, k, v))?;
            }
            if let Some(kwargs) = py_kwargs {
                kwargs.items().iter().try_for_each(|tup| {
                    let (k, v): (HeaderName, HeaderValue) = tup.extract()?;
                    insert(map, k, v)
                })?;
            }
            Ok(())
        })
    }

    fn setdefault(&self, key: HeaderName, default: HeaderValue) -> PyResult<HeaderValue> {
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

    // Additional methods

    fn keys_len(&self) -> PyResult<usize> {
        self.ref_map(|map| Ok(map.keys_len()))
    }

    fn getall(&self, key: &str) -> PyResult<Vec<HeaderValue>> {
        self.get_all(key)
    }

    #[pyo3(signature = (key, value, *, is_sensitive=false))]
    fn insert(&self, key: HeaderName, value: HeaderValue, is_sensitive: bool) -> PyResult<Vec<HeaderValue>> {
        let mut value = value.0;
        value.set_sensitive(is_sensitive);

        self.mut_map(|map| match map.try_entry(key.0) {
            Ok(Entry::Occupied(mut entry)) => Ok(entry.insert_mult(value).map(HeaderValue).collect()),
            Ok(Entry::Vacant(entry)) => entry
                .try_insert_entry(value)
                .map(|_| vec![])
                .map_err(|e| PyRuntimeError::new_err(e.to_string())),
            Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
        })
    }

    #[pyo3(signature = (key, value, *, is_sensitive=false))]
    fn append(&self, key: HeaderName, value: HeaderValue, is_sensitive: bool) -> PyResult<bool> {
        let mut value = value.0;
        value.set_sensitive(is_sensitive);

        self.mut_map(|map| {
            map.try_append(key.0, value)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
        })
    }

    fn extend(&self, other: KeyValPairs) -> PyResult<()> {
        self.mut_map(|map| HeaderMap::extend_inner(map, other))
    }

    #[pyo3(signature = (key, default=PopArg::NotPresent(ellipsis())))]
    fn popall<'py>(&self, py: Python<'py>, key: &str, default: PopArg<'py>) -> PyResult<Bound<'py, PyAny>> {
        let key = match http::HeaderName::try_from(key) {
            Ok(name) => name,
            Err(_) => return default.res(key.to_string()), // Invalid key, can not be present in map
        };

        self.mut_map(|map| match map.try_entry(key) {
            Ok(Entry::Occupied(entry)) => entry
                .remove_entry_mult()
                .1
                .map(HeaderValue)
                .collect::<Vec<_>>()
                .into_bound_py_any(py),
            Ok(Entry::Vacant(entry)) => default.res(entry.into_key().to_string()),
            Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
        })
    }

    fn dict_multi_value<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        self.ref_map(|map| Self::dict_multi_value_inner(map, py, false))
    }

    fn copy(&self) -> PyResult<Self> {
        self.__copy__()
    }

    fn __copy__(&self) -> PyResult<Self> {
        self.ref_map(|map| Ok(HeaderMap::from(map.clone())))
    }

    fn __str__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        self.ref_map(|map| Self::dict_multi_value_inner(map, py, true)?.str())
    }

    fn __repr__(&self, py: Python) -> PyResult<String> {
        let repr = self.ref_map(|map| Self::dict_multi_value_inner(map, py, true)?.repr())?;
        Ok(format!("HeaderMap({})", repr.to_str()?))
    }
}

impl HeaderMap {
    pub fn get_one(&self, key: &str) -> PyResult<Option<HeaderValue>> {
        self.ref_map(|map| Ok(map.get(key).map(|v| HeaderValue(v.clone()))))
    }

    pub fn get_all(&self, key: &str) -> PyResult<Vec<HeaderValue>> {
        self.ref_map(|map| Ok(map.get_all(key).into_iter().map(|v| HeaderValue(v.clone())).collect()))
    }

    pub fn try_clone(&self) -> PyResult<Self> {
        self.ref_map(|map| Ok(HeaderMap::from(map.clone())))
    }

    pub fn try_clone_inner(&self) -> PyResult<http::HeaderMap> {
        self.ref_map(|map| Ok(map.clone()))
    }

    pub fn try_take_inner(&self) -> PyResult<http::HeaderMap> {
        self.safe_lock()?
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("HeaderMap was already consumed"))
    }

    pub fn clone_arc(&self) -> Self {
        HeaderMap(Arc::clone(&self.0))
    }

    pub fn ref_map<U, F: FnOnce(&http::HeaderMap) -> PyResult<U>>(&self, f: F) -> PyResult<U> {
        match self.safe_lock()?.as_ref() {
            Some(map) => f(map),
            None => Err(PyRuntimeError::new_err("HeaderMap was already consumed")),
        }
    }

    pub fn mut_map<U, F: FnOnce(&mut http::HeaderMap) -> PyResult<U>>(&self, f: F) -> PyResult<U> {
        match self.safe_lock()?.as_mut() {
            Some(map) => f(map),
            None => Err(PyRuntimeError::new_err("HeaderMap was already consumed")),
        }
    }

    fn safe_lock(&self) -> PyResult<std::sync::MutexGuard<'_, Inner>> {
        self.0
            .lock()
            .map_err(|_| PyRuntimeError::new_err("HeaderMap mutex poisoned"))
    }

    pub fn keys_once_deque(&self) -> PyResult<VecDeque<HeaderName>> {
        self.ref_map(|map| Ok(map.keys().map(|k| HeaderName(k.clone())).collect()))
    }

    pub fn keys_mult_deque(&self) -> PyResult<VecDeque<HeaderName>> {
        self.ref_map(|map| Ok(map.iter().map(|(k, _)| HeaderName(k.clone())).collect()))
    }

    pub fn get_all_extend_to_deque(&self, key: &str, deque: &mut VecDeque<HeaderValue>) -> PyResult<()> {
        self.ref_map(|map| {
            deque.extend(map.get_all(key).into_iter().map(|v| HeaderValue(v.clone())));
            Ok(())
        })
    }

    fn pop_inner<'py>(
        map: &mut http::HeaderMap,
        py: Python<'py>,
        key: &http::HeaderName,
        default: PopArg<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (res, new_vals) = match map.try_entry(key) {
            Ok(Entry::Occupied(entry)) => {
                let mut vals = entry.remove_entry_mult().1;
                // Remove the first value, add back the rest (pop first)
                let first = vals
                    .next()
                    .ok_or_else(|| PyRuntimeError::new_err("Expected at least one value"))?;
                (HeaderValue(first), vals.collect::<Vec<_>>())
            }
            Ok(Entry::Vacant(entry)) => return default.res(entry.into_key().to_string()),
            Err(e) => return Err(PyRuntimeError::new_err(e.to_string())),
        };

        for val in new_vals {
            map.try_append(key.clone(), val)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        }

        res.into_bound_py_any(py)
    }

    fn extend_inner(map: &mut http::HeaderMap, other: KeyValPairs) -> PyResult<()> {
        other.for_each(|(k, v): (HeaderName, HeaderValue)| {
            map.try_append(k.0, v.0)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
                .map(|_| ())
        })
    }

    pub fn extend_into_inner(self, map: &mut http::HeaderMap) -> PyResult<()> {
        let mut prev_k: Option<http::HeaderName> = None;
        for (k, v) in self.try_take_inner()? {
            match k {
                Some(k) => {
                    prev_k = Some(k.clone());
                    map.try_append(k, v)
                }
                None => {
                    let k = prev_k
                        .as_ref()
                        .ok_or_else(|| PyRuntimeError::new_err("Expected prev key"))?;
                    map.try_append(k.clone(), v)
                }
            }
            .map(|_| ())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        }
        Ok(())
    }

    pub fn dict_multi_value_inner<'py>(
        map: &http::HeaderMap,
        py: Python<'py>,
        hide_sensitive: bool,
    ) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        for (key, value) in map.iter() {
            let key = key.as_str();
            let value = if hide_sensitive && value.is_sensitive() {
                intern!(py, "Sensitive").to_owned()
            } else {
                PyString::new(py, &HeaderValue::inner_str(value)?)
            };

            match dict.get_item(key)? {
                None => dict.set_item(key, value)?,
                Some(existing) => match existing.downcast_into_exact::<PyString>() {
                    Ok(existing) => dict.set_item(key, PyList::new(py, vec![existing, value])?)?,
                    Err(e) => e.into_inner().downcast_into_exact::<PyList>()?.append(value)?,
                },
            }
        }
        Ok(dict)
    }
}
impl From<http::HeaderMap> for HeaderMap {
    fn from(value: http::HeaderMap) -> Self {
        HeaderMap(Arc::new(Mutex::new(Some(value))))
    }
}

#[derive(FromPyObject)]
enum PopArg<'py> {
    #[allow(dead_code)]
    NotPresent(Py<PyEllipsis>),
    Value(Bound<'py, PyAny>),
}
impl<'py> PopArg<'py> {
    fn res(self, key: String) -> PyResult<Bound<'py, PyAny>> {
        match self {
            PopArg::Value(v) => Ok(v),
            PopArg::NotPresent(_) => Err(PyKeyError::new_err(key)),
        }
    }
}

impl<'py> FromPyObject<'py> for HeaderMap {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        if let Ok(map) = ob.downcast_exact::<HeaderMap>() {
            Ok(map.get().try_clone()?)
        } else {
            Ok(HeaderMap::new_py(Some(ob.extract::<KeyValPairs>()?))?)
        }
    }
}
