use pyo3::PyResult;
use pyo3::exceptions::PyRuntimeError;

pub struct Runtime {
    inner: Option<tokio::runtime::Handle>,
    close_tx: tokio::sync::mpsc::Sender<()>,
}
impl Runtime {
    pub fn start_new() -> PyResult<Self> {
        let (handle_tx, handle_rx) = std::sync::mpsc::channel::<PyResult<tokio::runtime::Handle>>();
        let (close_tx, mut close_rx) = tokio::sync::mpsc::channel::<()>(1);

        std::thread::spawn(move || {
            let rt_res = tokio::runtime::Builder::new_current_thread().enable_all().build();
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

        let handle = handle_rx
            .recv()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to recv tokio runtime: {}", e)))??;

        Ok(Runtime {
            inner: Some(handle),
            close_tx,
        })
    }

    pub fn spawn<F, T>(&self, future: F) -> PyResult<tokio::task::JoinHandle<T>>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        Ok(self.runtime()?.spawn(future))
    }

    fn runtime(&self) -> PyResult<&tokio::runtime::Handle> {
        self.inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Runtime has been dropped"))
    }

    pub fn close(&self) {
        let _ = self.close_tx.try_send(());
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.close_tx.try_send(());
    }
}
