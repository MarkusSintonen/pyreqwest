use crate::exceptions::{ClientClosedError, RequestPanicError};
use futures_util::FutureExt;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::exceptions::asyncio::CancelledError;
use pyo3::prelude::*;
use std::sync::LazyLock;

static GLOBAL_HANDLE: LazyLock<PyResult<InnerRuntime>> = LazyLock::new(|| {
    let (close_tx, close_rx) = tokio::sync::mpsc::channel::<()>(1);
    let handle = Runtime::new_handle(close_rx)?;
    Ok(InnerRuntime { handle, close_tx })
});

#[derive(Clone)]
pub struct RuntimeHandle(tokio::runtime::Handle);
impl RuntimeHandle {
    pub fn spawn<F, T>(&self, future: F) -> PyResult<tokio::task::JoinHandle<T>>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        Ok(self.0.spawn(future))
    }

    pub async fn spawn_handled<F, T>(&self, future: F, mut cancel: CancelHandle) -> PyResult<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let join_handle = self.spawn(future)?;

        tokio::select! {
            res = join_handle => res.map_err(|e| match e.try_into_panic() {
                Ok(panic_payload) => RequestPanicError::from_panic_payload("Request panicked", panic_payload),
                Err(e) => ClientClosedError::from_err("Runtime was closed", &e),
            }),
            _ = cancel.cancelled().fuse() => Err(CancelledError::new_err("Request was cancelled")),
        }
    }

    pub fn blocking_spawn<F, T>(&self, future: F) -> T
    where
        F: Future<Output = T> + Send,
        T: Send,
    {
        Python::attach(|py| py.detach(|| self.0.block_on(future)))
    }

    pub fn global_handle() -> PyResult<&'static Self> {
        let inner = GLOBAL_HANDLE
            .as_ref()
            .map_err(|e| Python::attach(|py| e.clone_ref(py)))?;
        Ok(&inner.handle)
    }
}

pub struct InnerRuntime {
    handle: RuntimeHandle,
    close_tx: tokio::sync::mpsc::Sender<()>,
}

#[pyclass(frozen)]
pub struct Runtime(InnerRuntime);
#[pymethods]
impl Runtime {
    #[new]
    pub fn new() -> PyResult<Self> {
        let (close_tx, close_rx) = tokio::sync::mpsc::channel::<()>(1);
        let handle = Runtime::new_handle(close_rx)?;
        Ok(Runtime(InnerRuntime { handle, close_tx }))
    }

    pub async fn close(&self) -> PyResult<()> {
        self.0
            .close_tx
            .send(())
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to close runtime: {}", e)))
    }
}
impl Runtime {
    pub fn handle(&self) -> &RuntimeHandle {
        &self.0.handle
    }

    fn new_handle(mut close_rx: tokio::sync::mpsc::Receiver<()>) -> PyResult<RuntimeHandle> {
        let (handle_tx, handle_rx) = std::sync::mpsc::channel::<PyResult<tokio::runtime::Handle>>();

        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .thread_name("pyreqwest-worker".to_string())
                .enable_all()
                .build()
                .map_err(|e| {
                    let _ = // :NOCOV_START:
                        handle_tx.send(Err(PyRuntimeError::new_err(format!("Failed to create tokio runtime: {}", e))));
                }) // :NOCOV_END:
                .map(|rt| {
                    rt.block_on(async {
                        let _ = handle_tx.send(Ok(tokio::runtime::Handle::current()));
                    });
                    let _ = rt.block_on(close_rx.recv());
                    rt.shutdown_background()
                })
        });

        let handle = handle_rx
            .recv()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to recv tokio runtime: {}", e)))??;
        Ok(RuntimeHandle(handle))
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.0.close_tx.try_send(());
    }
}
