use crate::internal::task_local::TaskLocal;
use crate::response::BaseResponse;
use bytes::Bytes;
use futures_util::FutureExt;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::exceptions::asyncio::CancelledError;
use pyo3::prelude::*;
use pyo3::pyclass::boolean_struct::True;
use pyo3::sync::MutexExt;
use pyo3::{PyClass, PyTraverseError, PyVisit, intern};
use std::pin::Pin;
use std::sync::{Mutex, MutexGuard};
use std::task::{Context, Poll};
use tokio::sync::oneshot;

type BytesOpt = Option<Bytes>;
type PyObj = Py<PyAny>;
type Extractor<T> = Box<dyn FnOnce(Python, PyResult<Bound<PyAny>>) -> PyResult<T> + Send + Sync>;

struct BridgeInner<T> {
    tx: Option<oneshot::Sender<PyResult<T>>>,
    extractor: Option<Extractor<T>>,
    coro: Option<Py<PyAny>>,
    event_loop: Option<Py<PyAny>>,
    task: Option<Py<PyAny>>,
}

impl<T> BridgeInner<T> {
    fn new(
        tx: oneshot::Sender<PyResult<T>>,
        extractor: Extractor<T>,
        coro: Bound<PyAny>,
        task_local: &TaskLocal,
    ) -> PyResult<Self> {
        Ok(Self {
            tx: Some(tx),
            extractor: Some(extractor),
            event_loop: Some(task_local.event_loop()?.clone_ref(coro.py())),
            coro: Some(coro.unbind()),
            task: None,
        })
    }

    fn init_task(&mut self, slf: &Bound<PyAny>, py: Python) -> PyResult<()> {
        let res = (|| -> PyResult<()> {
            let coro = self
                .coro
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Task was cancelled"))?;

            let task = self
                .event_loop
                .as_ref()
                .ok_or_else(|| PyRuntimeError::new_err("Expected event_loop"))?
                .bind(py)
                .call_method1(intern!(py, "create_task"), (coro,))?;

            let callback = slf.getattr(intern!(py, "on_done"))?;
            task.call_method1(intern!(py, "add_done_callback"), (callback,))?;

            self.task = Some(task.unbind());
            Ok(())
        })();

        if let Err(ref e) = res
            && let Some(tx) = self.tx.take()
        {
            let _ = tx.send(Err(e.clone_ref(py))); // :NOCOV
        }
        res
    }

    fn handle_done(&mut self, py: Python, task: Bound<PyAny>) -> PyResult<()> {
        let res = (|| -> PyResult<T> {
            self.task = None;
            let task_res = task.call_method0(intern!(py, "result"));
            let extractor = self
                .extractor
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Sender already consumed"))?;
            extractor(py, task_res)
        })();

        let _ = self
            .tx
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Task was cancelled"))?
            .send(res);
        Ok(())
    }

    fn cancel(&mut self, py: Python) -> PyResult<()> {
        self.coro = None;
        if let Some(task) = self.task.take() {
            task.bind(py).call_method0(intern!(py, "cancel"))?;
        }
        Ok(())
    }

    fn __traverse__(&self, visit: &PyVisit<'_>) -> Result<(), PyTraverseError> {
        self.coro.as_ref().map(|v| visit.call(v)).transpose()?;
        self.event_loop.as_ref().map(|v| visit.call(v)).transpose()?;
        self.task.as_ref().map(|v| visit.call(v)).transpose()?;
        Ok(())
    }

    fn __clear__(&mut self) {
        self.coro = None;
        self.event_loop = None;
        self.task = None;
    }
}

pub trait CoroTaskInitializer<T> {
    fn new<'py>(
        tx: oneshot::Sender<PyResult<T>>,
        coro: Bound<'py, PyAny>,
        extractor: Extractor<T>,
        task_local: &TaskLocal,
    ) -> PyResult<Bound<'py, Self>>
    where
        Self: Sized;
    fn cancel(&self, py: Python) -> PyResult<()>;
}

pub struct CoroWaiter<T, I> {
    rx: oneshot::Receiver<PyResult<T>>,
    task_initializer: Py<I>,
    cancel_handle: Option<CancelHandle>,
}

impl<T, I> Unpin for CoroWaiter<T, I> {}

impl<T, I> Future for CoroWaiter<T, I>
where
    I: PyClass<Frozen = True> + CoroTaskInitializer<T> + Sync,
{
    type Output = PyResult<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(cancel_handle) = self.cancel_handle.as_mut() {
            match cancel_handle.poll_cancelled(cx) {
                Poll::Ready(_) => {
                    return match Python::attach(|py| self.task_initializer.get().cancel(py)) {
                        Ok(()) => Poll::Ready(Err(CancelledError::new_err("Task was cancelled"))),
                        Err(e) => Poll::Ready(Err(e)),
                    };
                }
                Poll::Pending => {}
            }
        }

        match self.rx.poll_unpin(cx) {
            Poll::Ready(ready) => Poll::Ready(
                ready
                    .map_err(|e| PyRuntimeError::new_err(format!("Failed to receive task result: {}", e)))
                    .flatten(),
            ),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T, I> CoroWaiter<T, I>
where
    I: PyClass<Frozen = True> + CoroTaskInitializer<T> + Sync,
{
    pub fn new(
        coro: Bound<PyAny>,
        extractor: Extractor<T>,
        task_local: &TaskLocal,
        cancel_handle: Option<CancelHandle>,
    ) -> PyResult<Self> {
        let py = coro.py();

        let (tx, rx) = oneshot::channel();
        let task_initializer = I::new(tx, coro, extractor, task_local)?;

        let init_callback = task_initializer.as_any().getattr(intern!(py, "on_init"))?;
        task_local.event_loop_call_soon(&init_callback)?;

        Ok(Self {
            rx,
            task_initializer: task_initializer.unbind(),
            cancel_handle,
        })
    }
}

macro_rules! create_asyncio_bridge {
    ($bridge_name: ident, $waiter_name: ident, $res_type: ident) => {
        #[pyclass(frozen)]
        pub struct $bridge_name(Mutex<BridgeInner<$res_type>>);

        #[pymethods]
        impl $bridge_name {
            fn on_init(slf: Bound<Self>, py: Python) -> PyResult<()> {
                slf.get().lock(py)?.init_task(slf.as_any(), py)
            }

            fn on_done(&self, py: Python, task: Bound<PyAny>) -> PyResult<()> {
                self.lock(py)?.handle_done(py, task)
            }

            fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
                if let Ok(inner) = self.0.try_lock() {
                    inner.__traverse__(&visit)?;
                }
                Ok(())
            }

            fn __clear__(&self) {
                let _ = self.0.try_lock().map(|mut inner| inner.__clear__());
            }
        }
        impl $bridge_name {
            fn lock(&self, py: Python) -> PyResult<MutexGuard<'_, BridgeInner<$res_type>>> {
                self.0
                    .lock_py_attached(py)
                    .map_err(|_| PyRuntimeError::new_err("mutex poisoned"))
            }
        }

        impl CoroTaskInitializer<$res_type> for $bridge_name {
            fn new<'py>(
                tx: oneshot::Sender<PyResult<$res_type>>,
                coro: Bound<'py, PyAny>,
                extractor: Extractor<$res_type>,
                task_local: &TaskLocal,
            ) -> PyResult<Bound<'py, Self>> {
                let py = coro.py();
                let inner = BridgeInner::new(tx, extractor, coro, task_local)?;
                Bound::new(py, Self(Mutex::new(inner)))
            }

            fn cancel(&self, py: Python) -> PyResult<()> {
                self.lock(py)?.cancel(py)
            }
        }

        pub type $waiter_name = CoroWaiter<$res_type, $bridge_name>;
    };
}

create_asyncio_bridge!(BytesBridge, BytesCoroWaiter, BytesOpt);
create_asyncio_bridge!(AnyBridge, AnyCoroWaiter, PyObj);
create_asyncio_bridge!(ResponseBridge, ResponseCoroWaiter, BaseResponse);
