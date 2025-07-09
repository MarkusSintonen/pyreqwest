use futures_util::FutureExt;
use pyo3::PyResult;
use pyo3::exceptions::PyRuntimeError;

pub struct Runtime {
    inner: Option<tokio::runtime::Handle>,
    close_tx: tokio::sync::mpsc::Sender<()>,
    shutdown_rx: futures_util::future::Shared<tokio::sync::oneshot::Receiver<()>>,
}
impl Runtime {
    pub fn start_new() -> PyResult<Self> {
        let (handle_tx, handle_rx) = std::sync::mpsc::channel::<PyResult<tokio::runtime::Handle>>();
        let (close_tx, mut close_rx) = tokio::sync::mpsc::channel::<()>(1);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        std::thread::spawn(move || {
            let rt_res = tokio::runtime::Builder::new_current_thread().enable_all().build();
            match rt_res {
                Ok(rt) => {
                    rt.block_on(async {
                        handle_tx.send(Ok(tokio::runtime::Handle::current())).unwrap();
                    });
                    let _ = rt.block_on(close_rx.recv());
                    rt.shutdown_background();
                    let _ = shutdown_tx.send(());
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
            shutdown_rx: shutdown_rx.shared(),
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
        if self.shutdown_rx.peek().is_some() {
            return Err(PyRuntimeError::new_err("Runtime has been shut down"));
        }

        self.inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Runtime has been dropped"))
    }

    pub async fn close(&self) {
        let _ = self.close_tx.try_send(());
        let _ = self.shutdown_rx.clone().await;
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.close_tx.try_send(());
    }
}
