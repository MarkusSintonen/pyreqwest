use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

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

    fn text(slf: PyRefMut<Self>, name: String, value: String) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.text(name, value)))
    }

    fn boundary(&self) -> PyResult<&str> {
        Ok(self.inner_ref()?.boundary())
    }
}
impl Form {
    fn apply<F>(mut slf: PyRefMut<Self>, fun: F) -> PyResult<PyRefMut<Self>>
    where
        F: FnOnce(reqwest::multipart::Form) -> PyResult<reqwest::multipart::Form>,
    {
        let builder = slf
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Form was already built"))?;
        slf.inner = Some(fun(builder)?);
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
