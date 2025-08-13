use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use std::sync::LazyLock;

static GLOBAL_HANDLE: LazyLock<PyResult<InnerRuntime>> = LazyLock::new(|| {
    let (close_tx, close_rx) = tokio::sync::mpsc::channel::<()>(1);
    let handle = Runtime::new_handle(None, close_rx)?;
    Ok(InnerRuntime { handle, close_tx })
});

pub struct InnerRuntime {
    handle: tokio::runtime::Handle,
    close_tx: tokio::sync::mpsc::Sender<()>,
}

#[pyclass]
pub struct Runtime(InnerRuntime);
#[pymethods]
impl Runtime {
    #[new]
    #[pyo3(signature = (/, thread_name=None))]
    pub fn new(thread_name: Option<String>) -> PyResult<Self> {
        let (close_tx, close_rx) = tokio::sync::mpsc::channel::<()>(1);
        let handle = Runtime::new_handle(thread_name, close_rx)?;
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
    pub fn global_handle() -> PyResult<&'static tokio::runtime::Handle> {
        let inner = GLOBAL_HANDLE
            .as_ref()
            .map_err(|e| Python::with_gil(|py| e.clone_ref(py)))?;
        Ok(&inner.handle)
    }

    pub fn handle(&self) -> &tokio::runtime::Handle {
        &self.0.handle
    }

    fn new_handle(
        thread_name: Option<String>,
        mut close_rx: tokio::sync::mpsc::Receiver<()>,
    ) -> PyResult<tokio::runtime::Handle> {
        let (handle_tx, handle_rx) = std::sync::mpsc::channel::<PyResult<tokio::runtime::Handle>>();

        std::thread::spawn(move || {
            let rt_res = tokio::runtime::Builder::new_current_thread()
                .thread_name(thread_name.unwrap_or("pyreqwest-worker".to_string()))
                .enable_all()
                .build();

            match rt_res {
                Ok(rt) => {
                    rt.block_on(async {
                        handle_tx.send(Ok(tokio::runtime::Handle::current())).unwrap();
                    });
                    let _ = rt.block_on(close_rx.recv());
                    rt.shutdown_background();
                }
                Err(e) => handle_tx
                    .send(Err(PyRuntimeError::new_err(format!("Failed to create tokio runtime: {}", e))))
                    .unwrap(),
            }
        });

        handle_rx
            .recv()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to recv tokio runtime: {}", e)))?
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.0.close_tx.try_send(());
    }
}
