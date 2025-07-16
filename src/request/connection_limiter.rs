use crate::exceptions::PoolTimeoutError;
use pyo3::PyResult;
use pyo3::exceptions::PyRuntimeError;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Clone)]
pub struct ConnectionLimiter {
    semaphore: Arc<Semaphore>,
    timeout: Option<Duration>,
}

impl ConnectionLimiter {
    pub fn new(limit: usize, timeout: Option<Duration>) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(limit)),
            timeout,
        }
    }

    pub async fn limit_connections(&self) -> PyResult<OwnedSemaphorePermit> {
        let permit = if let Some(timeout) = self.timeout {
            tokio::time::timeout(timeout, self.semaphore.clone().acquire_owned())
                .await
                .map_err(|_| PoolTimeoutError::new_err("Timeout acquiring semaphore", None))?
        } else {
            self.semaphore.clone().acquire_owned().await
        };
        permit.map_err(|e| PyRuntimeError::new_err(format!("Failed to acquire semaphore: {}", e)))
    }
}
