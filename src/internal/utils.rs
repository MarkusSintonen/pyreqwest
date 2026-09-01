use crate::internal::types::HeaderValue;
use base64::prelude::BASE64_STANDARD;
use base64::write::EncoderWriter;
use bytes::Bytes;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyEllipsis, PyList, PyMapping, PySequence, PyTuple};
use std::io::Write;
use std::str::FromStr;

pub fn ellipsis() -> Py<PyEllipsis> {
    Python::attach(|py| PyEllipsis::get(py).to_owned().unbind())
}

pub enum KeyValPairs<'py> {
    Mapping(Bound<'py, PyMapping>),
    List(Bound<'py, PyList>),
    Tuple(Bound<'py, PyTuple>),
    Sequence(Bound<'py, PySequence>),
}

impl<'py> FromPyObject<'_, 'py> for KeyValPairs<'py> {
    type Error = PyErr;
    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        if let Ok(v) = obj.cast_exact::<PyDict>() {
            return Ok(KeyValPairs::Mapping(v.as_mapping().clone()));
        }
        if let Ok(v) = obj.cast_exact::<PyList>() {
            return Ok(KeyValPairs::List(v.to_owned()));
        }
        if let Ok(v) = obj.cast_exact::<PyTuple>() {
            return Ok(KeyValPairs::Tuple(v.to_owned()));
        }
        if let Ok(v) = obj.cast::<PyMapping>() {
            return Ok(KeyValPairs::Mapping(v.to_owned()));
        }
        if let Ok(v) = obj.cast::<PyList>() {
            return Ok(KeyValPairs::List(v.to_owned()));
        }
        if let Ok(v) = obj.cast::<PyTuple>() {
            return Ok(KeyValPairs::Tuple(v.to_owned()));
        }
        if let Ok(v) = obj.cast::<PySequence>() {
            return Ok(KeyValPairs::Sequence(v.to_owned()));
        }
        Err(invalid_key_val_pairs(obj))
    }
}

#[cold]
fn invalid_key_val_pairs(obj: Borrowed<'_, '_, PyAny>) -> PyErr {
    let type_name = obj.get_type();
    let type_name = match type_name.name() {
        Ok(name) => name.to_string(),
        Err(e) => return e,
    };
    PyTypeError::new_err(format!(
        "failed to extract key-value pairs, '{type_name}' object is not a Mapping or a Sequence of (key, value) pairs"
    ))
}

impl<'py> KeyValPairs<'py> {
    pub fn for_each<F, K, V>(self, ctx: &str, mut f: F) -> PyResult<()>
    where
        F: FnMut((K, V)) -> PyResult<()>,
        for<'a> K: FromPyObject<'a, 'py>,
        for<'a> V: FromPyObject<'a, 'py>,
    {
        fn extract_error<'py, T>(v: &Bound<'py, PyAny>, ctx: &str, part: &str) -> PyResult<()>
        where
            for<'a> T: FromPyObject<'a, 'py>,
        {
            v.extract::<T>()
                .map_err(Into::into)
                .map_err(|e| {
                    let err = PyValueError::new_err(format!("Invalid {} {}: {}", ctx, part, v));
                    err.set_cause(v.py(), Some(e));
                    err
                })
                .map(|_| ())
        }

        fn error<'py, K, V>(v: Bound<'py, PyAny>, ctx: &str) -> PyResult<()>
        where
            for<'a> K: FromPyObject<'a, 'py>,
            for<'a> V: FromPyObject<'a, 'py>,
        {
            let tup = v.extract::<(Bound<'py, PyAny>, Bound<'py, PyAny>)>()?;
            extract_error::<K>(&tup.0, ctx, "key")?;
            extract_error::<V>(&tup.1, ctx, "value")
        }

        fn kv<'py, K, V>(v: Bound<'py, PyAny>, ctx: &str) -> PyResult<(K, V)>
        where
            for<'a> K: FromPyObject<'a, 'py>,
            for<'a> V: FromPyObject<'a, 'py>,
        {
            v.extract::<(K, V)>()
                .map_err(|e| error::<K, V>(v, ctx).err().unwrap_or(e))
        }

        match self {
            KeyValPairs::Mapping(v) => v.items()?.iter().try_for_each(|v| f(kv::<K, V>(v, ctx)?)),
            KeyValPairs::List(v) if v.as_any().is_exact_instance_of::<PyList>() => {
                v.iter().try_for_each(|v| f(kv::<K, V>(v, ctx)?))
            }
            KeyValPairs::List(v) => v.try_iter()?.try_for_each(|v| f(kv::<K, V>(v?, ctx)?)),
            KeyValPairs::Tuple(v) => v.iter().try_for_each(|v| f(kv::<K, V>(v, ctx)?)),
            KeyValPairs::Sequence(v) => v.try_iter()?.try_for_each(|v| f(kv::<K, V>(v?, ctx)?)),
        }
    }

    pub fn into_vec<K, V>(self, ctx: &str) -> PyResult<Vec<(K, V)>>
    where
        for<'a> K: FromPyObject<'a, 'py>,
        for<'a> V: FromPyObject<'a, 'py>,
    {
        let mut res = Vec::with_capacity(self.len()?);
        self.for_each(ctx, |(key, value)| {
            res.push((key, value));
            Ok(())
        })?;
        Ok(res)
    }

    pub fn len(&self) -> PyResult<usize> {
        match self {
            KeyValPairs::Mapping(v) => v.len(),
            KeyValPairs::List(v) => Ok(v.len()),
            KeyValPairs::Tuple(v) => Ok(v.len()),
            KeyValPairs::Sequence(v) => v.len(),
        }
    }
}

pub fn basic_auth(username: String, password: Option<String>) -> PyResult<http::HeaderValue> {
    let mut buf = b"Basic ".to_vec();
    {
        let mut encoder = EncoderWriter::new(&mut buf, &BASE64_STANDARD);
        let _ = write!(encoder, "{username}:");
        if let Some(password) = password {
            let _ = write!(encoder, "{password}");
        }
    }
    let mut header =
        http::HeaderValue::from_maybe_shared(Bytes::from(buf)).map_err(|e| PyValueError::new_err(e.to_string()))?;
    header.set_sensitive(true);
    Ok(header)
}

pub fn bearer_auth(token: String) -> PyResult<http::HeaderValue> {
    let mut header = HeaderValue::from_str(&format!("Bearer {token}"))?.0;
    header.set_sensitive(true);
    Ok(header)
}
