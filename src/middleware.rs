use crate::asyncio::py_coro_waiter;
use crate::http_types::{Extensions, HeaderMapExt, StatusCodeExt, UrlExt, VersionExt};
use crate::request_wrapper::RequestWrapper;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::intern;
use pyo3::prelude::*;
use pythonize::depythonize;
use serde::Deserialize;
use std::sync::Arc;

#[pyclass]
pub struct ResponseWrapper {
    response: Option<reqwest::Response>,
}

#[derive(Deserialize)]
struct ResponseMock {
    status_code: Option<StatusCodeExt>,
    headers: Option<HeaderMapExt>,
    version: Option<VersionExt>,
    body: Option<Vec<u8>>,
    extensions: Option<Extensions>,
}

#[pyclass]
pub struct Next {
    client: Arc<reqwest::Client>,
    middlewares: Arc<Vec<Py<PyAny>>>,
    current: usize,
}
#[pymethods]
impl Next {
    async fn run(&self, request: Py<RequestWrapper>) -> PyResult<Py<ResponseWrapper>> {
        if self.current < self.middlewares.len() {
            let cur = &self.middlewares[self.current];
            let next = Next {
                client: self.client.clone(),
                middlewares: self.middlewares.clone(),
                current: self.current + 1,
            };

            let fut = Python::with_gil(|py| {
                let coro = cur.bind(py).call_method1(intern!(py, "handle"), (request, next))?;
                py_coro_waiter(py, coro)
            })?;

            let res = fut.await?;

            Python::with_gil(|py| Ok::<_, PyErr>(res.into_bound(py).downcast_into_exact::<ResponseWrapper>()?.unbind()))
        } else {
            let resp = RequestWrapper::py_execute(request, &self.client).await?;
            Python::with_gil(|py| Py::new(py, ResponseWrapper { response: Some(resp) }))
        }
    }

    fn create_response<'py>(&mut self, py: Python<'py>, response_mock: ResponseMock) -> PyResult<Py<ResponseWrapper>> {
        let resp: reqwest::Response = response_mock.try_into()?;
        Py::new(py, ResponseWrapper { response: Some(resp) })
    }
}
impl Next {
    pub async fn process(
        client: Arc<reqwest::Client>,
        middlewares: Arc<Vec<Py<PyAny>>>,
        request: RequestWrapper,
    ) -> PyResult<reqwest::Response> {
        let resp = Next {
            client,
            middlewares,
            current: 0,
        }
        .run(Python::with_gil(|py| Py::new(py, request))?)
        .await?;

        Python::with_gil(|py| {
            resp.try_borrow_mut(py)?
                .response
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Response was already consumed"))
        })
    }
}

#[pymethods]
impl ResponseWrapper {
    fn get_status(&self) -> PyResult<StatusCodeExt> {
        Ok(self.try_get_response()?.status().into())
    }

    fn get_url(&self) -> PyResult<UrlExt> {
        self.try_get_response()?.url().clone().try_into()
    }

    fn get_version(&self) -> PyResult<VersionExt> {
        Ok(self.try_get_response()?.version().into())
    }

    fn get_headers(&self) -> PyResult<HeaderMapExt> {
        Ok(self.try_get_response()?.headers().clone().into())
    }

    fn set_headers(&mut self, value: HeaderMapExt) -> PyResult<()> {
        *self.try_mut_response()?.headers_mut() = value.0;
        Ok(())
    }

    fn get_extensions(&self) -> PyResult<Extensions> {
        Ok(self.try_get_response()?.extensions().into())
    }

    fn set_extensions(&mut self, value: Extensions) -> PyResult<()> {
        self.try_mut_response()?.extensions_mut().insert(value);
        Ok(())
    }
}
impl ResponseWrapper {
    fn try_get_response(&self) -> PyResult<&reqwest::Response> {
        self.response
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Response was already consumed"))
    }

    fn try_mut_response(&mut self) -> PyResult<&mut reqwest::Response> {
        self.response
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("Response was already consumed"))
    }
}

impl<'py> FromPyObject<'py> for ResponseMock {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(depythonize(ob)?)
    }
}
impl TryInto<reqwest::Response> for ResponseMock {
    type Error = PyErr;
    fn try_into(mut self) -> PyResult<reqwest::Response> {
        let mut res = http::Response::builder();
        if let Some(status_code) = self.status_code.take() {
            res = res.status(status_code.0);
        }
        if let Some(headers) = self.headers.take() {
            res.headers_mut().map(|h| *h = headers.0);
        }
        if let Some(version) = self.version.take() {
            res = res.version(version.0);
        }
        if let Some(extensions) = self.extensions.take() {
            res = res.extension(extensions);
        }
        let res = res
            .body(self.body.take().unwrap_or_default())
            .map_err(|e| PyValueError::new_err(format!("Failed to build response: {}", e)))?;
        Ok(reqwest::Response::from(res))
    }
}
