use crate::allow_threads::AllowThreads;
use crate::request::{Request, RequestBody};
use crate::response::{Response, SyncResponse};
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyType;
use pyo3::{PyTraverseError, PyVisit};

#[pyclass(extends=Request)]
pub struct StreamRequest(Option<Py<Response>>);

#[pyclass(extends=Request)]
pub struct SyncStreamRequest(Option<Py<SyncResponse>>);

#[pymethods]
impl StreamRequest {
    async fn __aenter__(slf: Py<Self>, #[pyo3(cancel_handle)] cancel: CancelHandle) -> PyResult<Py<Response>> {
        let resp = AllowThreads(Request::send_inner(slf.as_any(), cancel)).await?;

        Python::attach(|py| -> PyResult<_> {
            let response = Response::new_py(py, resp)?;
            slf.try_borrow_mut(py)?.0 = Some(response.clone_ref(py));
            Ok(response)
        })
    }

    async fn __aexit__(
        slf: Py<Self>,
        _exc_type: Py<PyAny>,
        _exc_val: Py<PyAny>,
        _traceback: Py<PyAny>,
    ) -> PyResult<()> {
        let body = Python::attach(|py| {
            slf.try_borrow_mut(py)?
                .0
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Must be used as a context manager"))?
                .into_bound(py)
                .into_super()
                .try_borrow_mut()?
                .take_body_reader()
        })?;
        AllowThreads(body.close()).await;
        Ok(())
    }

    fn __copy__(slf: PyRef<Self>, py: Python) -> PyResult<Py<Self>> {
        Self::new_py(py, slf.as_super().try_clone_inner(py, None)?)
    }

    #[classmethod]
    pub fn from_request_and_body(
        _cls: &Bound<'_, PyType>,
        py: Python,
        request: Bound<Self>,
        body: Option<Py<RequestBody>>,
    ) -> PyResult<Py<Self>> {
        Self::new_py(py, request.try_borrow()?.as_super().try_clone_inner(py, body)?)
    }

    // :NOCOV_START
    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        let Some(resp) = self.0.as_ref() else {
            return Ok(());
        };
        visit.call(resp)
    }

    fn __clear__(&mut self) {
        self.0.take();
    } // :NOCOV_END
}
impl StreamRequest {
    pub fn new_py(py: Python, inner: Request) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self(None)))
    }
}

#[pymethods]
impl SyncStreamRequest {
    fn __enter__(slf: Bound<Self>) -> PyResult<Py<SyncResponse>> {
        let resp = Request::blocking_send_inner(slf.as_unbound().as_any())?;

        let response = SyncResponse::new_py(slf.py(), resp)?;
        slf.try_borrow_mut()?.0 = Some(response.clone_ref(slf.py()));
        Ok(response)
    }

    fn __exit__(
        &mut self,
        _exc_type: Py<PyAny>,
        _exc_val: Py<PyAny>,
        _traceback: Py<PyAny>,
        py: Python,
    ) -> PyResult<()> {
        let mut resp = self
            .0
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Must be used as a context manager"))?
            .into_bound(py)
            .into_super()
            .try_borrow_mut()?;
        let body = resp.take_body_reader()?;
        SyncResponse::runtime(&resp)?.blocking_spawn(body.close());
        Ok(())
    }

    fn __copy__(slf: PyRef<Self>, py: Python) -> PyResult<Py<Self>> {
        Self::new_py(py, slf.as_super().try_clone_inner(py, None)?)
    }

    #[classmethod]
    pub fn from_request_and_body(
        _cls: &Bound<'_, PyType>,
        py: Python,
        request: Bound<Self>,
        body: Option<Py<RequestBody>>,
    ) -> PyResult<Py<Self>> {
        Self::new_py(py, request.try_borrow()?.as_super().try_clone_inner(py, body)?)
    }

    // :NOCOV_START
    pub fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        let Some(resp) = self.0.as_ref() else {
            return Ok(());
        };
        visit.call(resp)
    }

    fn __clear__(&mut self) {
        self.0.take();
    } // :NOCOV_END
}
impl SyncStreamRequest {
    pub fn new_py(py: Python, inner: Request) -> PyResult<Py<Self>> {
        Py::new(py, PyClassInitializer::from(inner).add_subclass(Self(None)))
    }
}
