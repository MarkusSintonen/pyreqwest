use crate::http::header_map::header_map::HeaderMap;
use pyo3::exceptions::{PyRuntimeError, PyStopIteration, PyValueError};
use pyo3::{PyRef, PyResult, pyclass, pymethods};

#[pyclass]
pub struct HeaderMapItemsIter {
    map: HeaderMap,
    iter: http::header::Iter<'static, http::HeaderValue>,
    expected_invalidator: usize,
}
#[pymethods]
impl HeaderMapItemsIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self) -> PyResult<(&str, &str)> {
        let inner = self.map.inner.lock().unwrap();
        if inner.invalidator != self.expected_invalidator || inner.map.is_none() {
            return Err(PyRuntimeError::new_err("HeaderMap modified during iteration"));
        }
        match self.iter.next() {
            Some((name, value)) => {
                let value_str = value.to_str().map_err(|e| PyValueError::new_err(e.to_string()))?;
                Ok((name.as_str(), value_str))
            }
            None => Err(PyStopIteration::new_err("No more items")),
        }
    }
}
impl HeaderMapItemsIter {
    pub fn new(map: HeaderMap) -> PyResult<Self> {
        let (iter, expected_invalidator) = {
            let inner_guard = map.inner.lock().unwrap();
            let Some(map) = inner_guard.map.as_ref() else {
                return Err(PyRuntimeError::new_err("HeaderMapItemsIter was already consumed"));
            };
            let iter: http::header::Iter<'_, http::HeaderValue> = map.iter();
            // Safety: HeaderMap backing the iter is not dropped while the struct is alive as we hold the Arc-Mutex.
            // Also, the iterator is stopped when the HeaderMap is modified, which is checked in __next__.
            let iter: http::header::Iter<'static, http::HeaderValue> = unsafe { std::mem::transmute(iter) };
            (iter, inner_guard.invalidator)
        };
        Ok(HeaderMapItemsIter {
            map,
            iter,
            expected_invalidator,
        })
    }
}

#[pyclass]
pub struct HeaderMapKeysIter(HeaderMapItemsIter);
#[pymethods]
impl HeaderMapKeysIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&mut self) -> PyResult<&str> {
        self.0.__next__().map(|(key, _)| key)
    }
}
impl HeaderMapKeysIter {
    pub fn new(map: HeaderMap) -> PyResult<Self> {
        Ok(HeaderMapKeysIter(HeaderMapItemsIter::new(map)?))
    }
}

#[pyclass]
pub struct HeaderMapValuesIter(HeaderMapItemsIter);
#[pymethods]
impl HeaderMapValuesIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&mut self) -> PyResult<&str> {
        self.0.__next__().map(|(_, val)| val)
    }
}
impl HeaderMapValuesIter {
    pub fn new(map: HeaderMap) -> PyResult<Self> {
        Ok(HeaderMapValuesIter(HeaderMapItemsIter::new(map)?))
    }
}
