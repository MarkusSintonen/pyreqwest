use pyo3::prelude::*;
use pyo3::types::{PyEllipsis, PyList, PyMapping, PySequence, PyTuple};

pub fn ellipsis() -> Py<PyEllipsis> {
    Python::attach(|py| PyEllipsis::get(py).to_owned().unbind())
}

#[derive(FromPyObject)]
pub enum KeyValPairs<'py> {
    Mapping(Bound<'py, PyMapping>),
    List(Bound<'py, PyList>),
    Tuple(Bound<'py, PyTuple>),
    Sequence(Bound<'py, PySequence>),
}
impl<'py> KeyValPairs<'py> {
    pub fn for_each<F, K: FromPyObject<'py>, V: FromPyObject<'py>>(self, mut f: F) -> PyResult<()>
    where
        F: FnMut((K, V)) -> PyResult<()>,
    {
        fn extract_pair<'py, K: FromPyObject<'py>, V: FromPyObject<'py>>(item: Bound<'py, PyAny>) -> PyResult<(K, V)>
        where
            (K, V): FromPyObject<'py>,
        {
            let (key, value): (K, V) = item.extract()?;
            Ok((key, value))
        }

        match self {
            KeyValPairs::Mapping(v) => v.items()?.iter().try_for_each(|v| f(extract_pair(v)?)),
            KeyValPairs::List(v) => v.try_iter()?.try_for_each(|v| f(extract_pair(v?)?)),
            KeyValPairs::Tuple(v) => v.iter().try_for_each(|v| f(extract_pair(v)?)),
            KeyValPairs::Sequence(v) => v.try_iter()?.try_for_each(|v| f(extract_pair(v?)?)),
        }
    }

    pub fn into_vec<K: FromPyObject<'py>, V: FromPyObject<'py>>(self) -> PyResult<Vec<(K, V)>>
    where
        (K, V): FromPyObject<'py>,
    {
        let mut res = Vec::with_capacity(self.len()?);
        self.for_each(|(key, value)| {
            res.push((key, value));
            Ok(())
        })?;
        Ok(res)
    }

    fn len(&self) -> PyResult<usize> {
        match self {
            KeyValPairs::Mapping(v) => v.len(),
            KeyValPairs::List(v) => Ok(v.len()),
            KeyValPairs::Tuple(v) => Ok(v.len()),
            KeyValPairs::Sequence(v) => v.len(),
        }
    }
}
