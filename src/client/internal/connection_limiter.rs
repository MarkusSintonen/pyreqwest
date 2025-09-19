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
        let semaphore = Arc::new(Semaphore::new(limit));
        Self { semaphore, timeout }
    }

    pub async fn limit_connections(&self, request_timeout: Option<Duration>) -> PyResult<OwnedSemaphorePermit> {
        let timeout = match (self.timeout, request_timeout) {
            (Some(t1), Some(t2)) => Some(t1.min(t2)),
            (Some(t1), None) => Some(t1),
            (None, Some(t2)) => Some(t2),
            (None, None) => None,
        };

        match timeout {
            Some(timeout) => tokio::time::timeout(timeout, self.semaphore.clone().acquire_owned())
                .await
                .map_err(|e| PoolTimeoutError::from_err("Timeout acquiring semaphore", &e))?,
            None => self.semaphore.clone().acquire_owned().await,
        }
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to acquire semaphore: {}", e)))
    }
}
