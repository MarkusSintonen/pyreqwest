use crate::allow_threads::AllowThreads;
use crate::client::RuntimeHandle;
use crate::response::internal::{BodyReader, DEFAULT_READ_BUFFER_LIMIT};
use bytes::Bytes;
use pyo3::prelude::*;
use pyo3_bytes::PyBytes;
use tokio::sync::Mutex;

#[pyclass(subclass, frozen)]
pub struct BaseResponseBodyReader {
    inner: Mutex<BodyReader>,
    runtime: RuntimeHandle,
}

#[pyclass(extends=BaseResponseBodyReader, frozen)]
pub struct ResponseBodyReader;
#[pyclass(extends=BaseResponseBodyReader, frozen)]
pub struct SyncResponseBodyReader;

#[pymethods]
impl BaseResponseBodyReader {
    async fn bytes(&self) -> PyResult<PyBytes> {
        AllowThreads(async { self.bytes_inner().await.map(PyBytes::new) }).await
    }

    #[pyo3(signature = (amount=DEFAULT_READ_BUFFER_LIMIT))]
    async fn read(&self, amount: usize) -> PyResult<Option<PyBytes>> {
        AllowThreads(async { Ok(self.read_inner(amount).await?.map(PyBytes::new)) }).await
    }

    async fn read_chunk(&self) -> PyResult<Option<PyBytes>> {
        AllowThreads(async { Ok(self.inner.lock().await.next_chunk().await?.map(PyBytes::new)) }).await
    }
}
impl BaseResponseBodyReader {
    pub fn new(body_reader: BodyReader) -> Self {
        Self {
            runtime: body_reader.runtime().clone(),
            inner: Mutex::new(body_reader),
        }
    }

    pub async fn bytes_inner(&self) -> PyResult<Bytes> {
        self.inner.lock().await.bytes().await
    }

    pub async fn read_inner(&self, amount: usize) -> PyResult<Option<Bytes>> {
        self.inner.lock().await.read(amount).await
    }

    pub async fn close(&self) {
        self.inner.lock().await.close();
    }
}

impl ResponseBodyReader {
    pub fn new_py(py: Python, inner: BodyReader) -> PyResult<Py<Self>> {
        let base = BaseResponseBodyReader::new(inner);
        Py::new(py, PyClassInitializer::from(base).add_subclass(Self))
    }
}

#[pymethods]
impl SyncResponseBodyReader {
    fn bytes(slf: PyRef<Self>) -> PyResult<PyBytes> {
        Self::runtime(slf.as_ref()).blocking_spawn(slf.as_super().bytes())
    }

    #[pyo3(signature = (amount=DEFAULT_READ_BUFFER_LIMIT))]
    fn read(slf: PyRef<Self>, amount: usize) -> PyResult<Option<PyBytes>> {
        Self::runtime(slf.as_ref()).blocking_spawn(slf.as_super().read(amount))
    }

    fn read_chunk(slf: PyRef<Self>) -> PyResult<Option<PyBytes>> {
        Self::runtime(slf.as_ref()).blocking_spawn(slf.as_super().read_chunk())
    }
}
impl SyncResponseBodyReader {
    pub fn new_py(py: Python, inner: BodyReader) -> PyResult<Py<Self>> {
        let base = BaseResponseBodyReader::new(inner);
        Py::new(py, PyClassInitializer::from(base).add_subclass(Self))
    }

    fn runtime(slf: &BaseResponseBodyReader) -> &RuntimeHandle {
        &slf.runtime
    }
}
