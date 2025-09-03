use crate::allow_threads::AllowThreads;
use crate::client::Runtime;
use crate::multipart::Part;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use std::path::PathBuf;

#[pyclass]
pub struct Form {
    inner: Option<reqwest::multipart::Form>,
}
#[pymethods]
impl Form {
    #[new]
    fn new() -> Self {
        Form {
            inner: Some(reqwest::multipart::Form::new()),
        }
    }

    #[getter]
    fn boundary(&self) -> PyResult<&str> {
        Ok(self.inner_ref()?.boundary())
    }

    fn text(slf: PyRefMut<Self>, name: String, value: String) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.text(name, value)))
    }

    async fn file(
        slf: Py<Self>,
        name: String,
        path: PathBuf,
        #[pyo3(cancel_handle)] cancel: CancelHandle,
    ) -> PyResult<Py<Self>> {
        let fut = Runtime::global_handle()?.spawn(reqwest::multipart::Part::file(path), cancel);
        let part = AllowThreads(fut).await??;
        Python::attach(|py| {
            Self::apply(slf.try_borrow_mut(py)?, |builder| Ok(builder.part(name, part)))?;
            Ok(slf)
        })
    }

    fn part<'py>(slf: PyRefMut<'py, Self>, name: String, mut part: PyRefMut<Part>) -> PyResult<PyRefMut<'py, Self>> {
        let part = part.take_inner()?;
        Self::apply(slf, |builder| Ok(builder.part(name, part)))
    }

    fn percent_encode_path_segment(slf: PyRefMut<Self>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.percent_encode_path_segment()))
    }

    fn percent_encode_attr_chars(slf: PyRefMut<Self>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.percent_encode_attr_chars()))
    }

    fn percent_encode_noop(slf: PyRefMut<Self>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.percent_encode_noop()))
    }
}
impl Form {
    fn apply<F>(mut slf: PyRefMut<Self>, fun: F) -> PyResult<PyRefMut<Self>>
    where
        F: FnOnce(reqwest::multipart::Form) -> PyResult<reqwest::multipart::Form>,
        F: Send,
    {
        let builder = slf
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Form was already built"))?;
        slf.inner = Some(slf.py().detach(|| fun(builder))?);
        Ok(slf)
    }

    fn inner_ref(&self) -> PyResult<&reqwest::multipart::Form> {
        self.inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Request was already consumed"))
    }

    pub fn build(&mut self) -> PyResult<reqwest::multipart::Form> {
        self.inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Form was already built"))
    }
}
