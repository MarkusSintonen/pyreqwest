use crate::http::header_map::header_map::HeaderMap;
use crate::internal::types::{HeaderName, HeaderValue};
use pyo3::exceptions::{PyRuntimeError, PyStopIteration};
use pyo3::prelude::*;
use std::collections::VecDeque;

#[pyclass]
pub struct HeaderMapItemsIter {
    keys: VecDeque<HeaderName>,
    cur_values: VecDeque<HeaderValue>,
    map: HeaderMap,
}
#[pymethods]
impl HeaderMapItemsIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self) -> PyResult<(HeaderName, HeaderValue)> {
        if self.cur_values.is_empty() {
            if let Some(key) = self.keys.front() {
                self.map.get_all_extend_to_deque(key.0.as_str(), &mut self.cur_values)?;
                assert!(!self.cur_values.is_empty(), "Should have at least one value for a header key");
            } else {
                return Err(PyStopIteration::new_err("No more items"));
            }
        }

        let value = self
            .cur_values
            .pop_front()
            .ok_or_else(|| PyRuntimeError::new_err("Expected iter value"))?;
        let key = if self.cur_values.is_empty() {
            self.keys.pop_front() // Go to next key
        } else {
            self.keys.front().cloned()
        };
        let key = key.ok_or_else(|| PyRuntimeError::new_err("Expected iter key"))?;
        Ok((key, value))
    }
}
impl HeaderMapItemsIter {
    pub fn new(map: HeaderMap) -> PyResult<Self> {
        Ok(HeaderMapItemsIter {
            keys: map.keys_once_deque()?,
            cur_values: VecDeque::new(),
            map,
        })
    }
}

#[pyclass]
pub struct HeaderMapKeysIter(VecDeque<HeaderName>);
#[pymethods]
impl HeaderMapKeysIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&mut self) -> PyResult<HeaderName> {
        match self.0.pop_front() {
            Some(key) => Ok(key),
            None => Err(PyStopIteration::new_err("No more keys")),
        }
    }
}
impl HeaderMapKeysIter {
    pub fn new(map: &HeaderMap) -> PyResult<Self> {
        Ok(HeaderMapKeysIter(map.keys_mult_deque()?))
    }
}

#[pyclass]
pub struct HeaderMapValuesIter(HeaderMapItemsIter);
#[pymethods]
impl HeaderMapValuesIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&mut self) -> PyResult<HeaderValue> {
        self.0.__next__().map(|(_, val)| val)
    }
}
impl HeaderMapValuesIter {
    pub fn new(map: HeaderMap) -> PyResult<Self> {
        Ok(HeaderMapValuesIter(HeaderMapItemsIter::new(map)?))
    }
}
