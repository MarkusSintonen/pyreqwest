use crate::client::connection_limiter::ConnectionLimiter;
use crate::client::runtime;
use crate::exceptions::utils::map_send_error;
use crate::exceptions::{ClientClosedError, PoolTimeoutError};
use crate::http::Extensions;
use crate::response::{BodyConsumeConfig, Response};
use pyo3::PyResult;
use pyo3::coroutine::CancelHandle;
use tokio::sync::OwnedSemaphorePermit;
use tokio_util::sync::CancellationToken;

pub struct Spawner {
    client: reqwest::Client,
    runtime: runtime::Handle,
    connection_limiter: Option<ConnectionLimiter>,
    close_cancellation: CancellationToken,
}
impl Spawner {
    pub fn new(
        client: reqwest::Client,
        runtime: runtime::Handle,
        connection_limiter: Option<ConnectionLimiter>,
        close_cancellation: CancellationToken,
    ) -> Self {
        Self {
            client,
            runtime,
            connection_limiter,
            close_cancellation,
        }
    }

    pub async fn spawn_reqwest(
        &self,
        mut request: reqwest::Request,
        body_consume_config: BodyConsumeConfig,
        extensions: Option<Extensions>,
        cancel: CancelHandle,
    ) -> PyResult<Response> {
        let client = self.client.clone();
        let connection_limiter = self.connection_limiter.clone();

        let fut = async move {
            let permit = match connection_limiter.as_ref() {
                Some(lim) => Some(Self::limit_connections(lim, &mut request).await?),
                _ => None,
            };

            let mut resp = client.execute(request).await.map_err(map_send_error)?;

            extensions
                .map(|ext| ext.into_response(resp.extensions_mut()))
                .transpose()?;

            Response::initialize(resp, permit, body_consume_config).await
        };

        let fut = self.runtime.spawn(fut, cancel);

        tokio::select! {
            res = fut => res?,
            _ = self.close_cancellation.cancelled() => Err(ClientClosedError::from_causes("Client was closed", vec![]),)
        }
    }

    async fn limit_connections(
        connection_limiter: &ConnectionLimiter,
        request: &mut reqwest::Request,
    ) -> PyResult<OwnedSemaphorePermit> {
        let req_timeout = request.timeout().copied();
        let now = std::time::Instant::now();

        let permit = connection_limiter.limit_connections(req_timeout).await?;
        let elapsed = now.elapsed();
        if let Some(req_timeout) = req_timeout {
            if elapsed >= req_timeout {
                return Err(PoolTimeoutError::from_causes("Timeout acquiring semaphore", vec![]));
            } else {
                *request.timeout_mut() = Some(req_timeout - elapsed);
            }
        }

        Ok(permit)
    }
}
impl Clone for Spawner {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            runtime: self.runtime.clone(),
            connection_limiter: self.connection_limiter.clone(),
            close_cancellation: self.close_cancellation.child_token(),
        }
    }
}
