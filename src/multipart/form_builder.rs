use crate::allow_threads::AllowThreads;
use crate::client::Handle;
use crate::multipart::PartBuilder;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use std::path::PathBuf;

#[pyclass]
pub struct FormBuilder {
    inner: Option<reqwest::multipart::Form>,
    is_async: bool,
}
#[pymethods]
impl FormBuilder {
    #[new]
    fn new() -> Self {
        FormBuilder {
            inner: Some(reqwest::multipart::Form::new()),
            is_async: false,
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
        let fut = Handle::global_handle()?.spawn_handled(reqwest::multipart::Part::file(path), cancel);
        let part = AllowThreads(fut).await??;
        Python::attach(|py| {
            Self::apply(slf.try_borrow_mut(py)?, |builder| Ok(builder.part(name, part)))?;
            Ok(slf)
        })
    }

    fn blocking_file(slf: PyRefMut<Self>, name: String, path: PathBuf) -> PyResult<PyRefMut<Self>> {
        let part = Handle::global_handle()?.blocking_spawn(reqwest::multipart::Part::file(path))?;
        Self::apply(slf, |builder| Ok(builder.part(name, part)))
    }

    fn part<'py>(
        mut slf: PyRefMut<'py, Self>,
        name: String,
        mut part: PyRefMut<PartBuilder>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if !slf.is_async {
            slf.is_async = part.is_async();
        }
        let part = part.build()?;
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
impl FormBuilder {
    pub fn build(&mut self) -> PyResult<reqwest::multipart::Form> {
        self.inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Form was already built"))
    }

    pub fn is_async(&self) -> bool {
        self.is_async
    }

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
            .ok_or_else(|| PyRuntimeError::new_err("Form was already built"))
    }
}
