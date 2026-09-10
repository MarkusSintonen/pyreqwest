use crate::exceptions::{ClientClosedError, RequestPanicError};
use futures_util::FutureExt;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::exceptions::asyncio::CancelledError;
use pyo3::prelude::*;
use std::sync::{OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::thread;
use std::time::Duration;

#[derive(Default)]
struct RuntimeConfig {
    multithreaded: Option<bool>,
    worker_threads: Option<usize>,
    max_blocking_threads: Option<usize>,
    blocking_thread_keep_alive: Option<Duration>,
    multithreaded_initialized: bool,
}
impl RuntimeConfig {
    fn global() -> &'static RwLock<RuntimeConfig> {
        static RUNTIME_CONFIG: OnceLock<RwLock<RuntimeConfig>> = OnceLock::new();
        RUNTIME_CONFIG.get_or_init(|| RwLock::new(RuntimeConfig::default()))
    }

    fn read() -> PyResult<RwLockReadGuard<'static, RuntimeConfig>> {
        Self::global()
            .read()
            .map_err(|_| PyRuntimeError::new_err("Config lock poisoned"))
    }

    fn write() -> PyResult<RwLockWriteGuard<'static, RuntimeConfig>> {
        Self::global()
            .write()
            .map_err(|_| PyRuntimeError::new_err("Config lock poisoned"))
    }

    fn build_tokio_runtime(multi_thread: bool) -> PyResult<tokio::runtime::Runtime> {
        fn build(mut builder: tokio::runtime::Builder) -> PyResult<tokio::runtime::Runtime> {
            builder
                .thread_name("pyreqwest-worker".to_string())
                .enable_all()
                .build()
                .map_err(|e| PyRuntimeError::new_err(format!("Failed to create tokio runtime: {}", e)))
        }

        if multi_thread {
            let mut config = Self::write()?;
            let mut builder = tokio::runtime::Builder::new_multi_thread();
            if let Some(threads) = config.worker_threads {
                builder.worker_threads(threads);
            }
            if let Some(threads) = config.max_blocking_threads {
                builder.max_blocking_threads(threads);
            }
            if let Some(duration) = config.blocking_thread_keep_alive {
                builder.thread_keep_alive(duration);
            }
            let runtime = build(builder)?;
            config.multithreaded_initialized = true;
            Ok(runtime)
        } else {
            build(tokio::runtime::Builder::new_current_thread())
        }
    }
}

macro_rules! set_config_check_init {
    ($name:ident, $value:expr) => {
        let mut config = RuntimeConfig::write()?;
        if config.$name != $value && config.multithreaded_initialized {
            return Err(PyRuntimeError::new_err(
                "Multi-threaded runtime config can not be changed after the multi-threaded runtime has been initialized",
            ));
        }
        config.$name = $value;
    };
}

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
                Err(e) => ClientClosedError::from_err("Runtime was closed", &e), // :NOCOV
            }),
            _ = cancel.cancelled().fuse() => Err(CancelledError::new_err("Request was cancelled")),
        }
    }

    pub fn blocking_spawn<F, T>(&self, py: Python, future: F) -> T
    where
        F: Future<Output = T> + Send,
        T: Send,
    {
        py.detach(|| self.0.block_on(future))
    }

    pub fn global_handle(multithreaded: Option<bool>) -> PyResult<&'static Self> {
        static ST_GLOBAL_HANDLE: OnceLock<PyResult<InnerRuntime>> = OnceLock::new();
        static MT_GLOBAL_HANDLE: OnceLock<PyResult<InnerRuntime>> = OnceLock::new();

        let multithreaded = multithreaded.unwrap_or_else(|| {
            RuntimeConfig::read()
                .ok()
                .and_then(|config| config.multithreaded)
                .unwrap_or(MT_GLOBAL_HANDLE.get().is_some()) // Use MT if it already exists
        });

        let global_handle = if multithreaded {
            &MT_GLOBAL_HANDLE
        } else {
            &ST_GLOBAL_HANDLE
        };
        let runtime = global_handle
            .get_or_init(|| InnerRuntime::new(multithreaded))
            .as_ref()
            .map_err(|e| Python::attach(|py| e.clone_ref(py)))?;

        if runtime.owner_pid != std::process::id() {
            return Err(PyRuntimeError::new_err(
                "A pyreqwest runtime was initialized before fork(). \
                 The Tokio async runtime cannot be safely reused or restarted in a \
                 forked child process. To avoid this: do not create pyreqwest clients \
                 before forking, or use Python's 'spawn' multiprocessing start method \
                 instead of 'fork'.",
            ));
        }

        Ok(&runtime.handle)
    }
}

struct InnerRuntime {
    handle: RuntimeHandle,
    owner_pid: u32,
    close_tx: Option<tokio::sync::oneshot::Sender<()>>,
}
impl InnerRuntime {
    pub fn new(multi_thread: bool) -> PyResult<Self> {
        let (handle_tx, handle_rx) = std::sync::mpsc::channel::<PyResult<tokio::runtime::Handle>>();
        let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();

        thread::Builder::new()
            .name("pyreqwest".to_string())
            .spawn(move || {
                RuntimeConfig::build_tokio_runtime(multi_thread)
                    .map_err(|e| handle_tx.send(Err(e)))
                    .map(|rt| {
                        rt.block_on(async {
                            let _ = handle_tx.send(Ok(tokio::runtime::Handle::current()));
                        });
                        let _ = rt.block_on(close_rx);
                        rt.shutdown_background()
                    })
            })
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to spawn tokio runtime thread: {}", e)))?;

        let handle = handle_rx
            .recv()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to recv tokio runtime: {}", e)))??;

        Ok(Self {
            handle: RuntimeHandle(handle),
            owner_pid: std::process::id(),
            close_tx: Some(close_tx),
        })
    }
}
impl Drop for InnerRuntime {
    fn drop(&mut self) {
        let _ = self.close_tx.take().map(|tx| tx.send(()));
    }
}

#[pyfunction]
pub fn runtime_multithreaded_default(enable: Option<bool>) -> PyResult<()> {
    let mut config = RuntimeConfig::write()?;
    config.multithreaded = enable;
    Ok(())
}

#[pyfunction]
pub fn runtime_worker_threads(threads: Option<usize>) -> PyResult<()> {
    set_config_check_init!(worker_threads, threads);
    Ok(())
}

#[pyfunction]
pub fn runtime_max_blocking_threads(threads: Option<usize>) -> PyResult<()> {
    set_config_check_init!(max_blocking_threads, threads);
    Ok(())
}

#[pyfunction]
pub fn runtime_blocking_thread_keep_alive(duration: Option<Duration>) -> PyResult<()> {
    set_config_check_init!(blocking_thread_keep_alive, duration);
    Ok(())
}
