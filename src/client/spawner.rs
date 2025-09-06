use crate::client::connection_limiter::ConnectionLimiter;
use crate::client::runtime;
use crate::exceptions::utils::map_send_error;
use crate::exceptions::{ClientClosedError, PoolTimeoutError};
use crate::http::Extensions;
use crate::response::{BaseResponse, BlockingResponse, BodyConsumeConfig, Response};
use pyo3::coroutine::CancelHandle;
use pyo3::prelude::*;
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

    async fn spawn_reqwest_inner(mut request: SpawnRequestData, cancel: CancelHandle) -> PyResult<BaseResponse> {
        let spawner = &request.spawner;
        let client = spawner.client.clone();
        let connection_limiter = spawner.connection_limiter.clone();
        let runtime = spawner.runtime.clone();

        let fut = async move {
            let permit = match connection_limiter.as_ref() {
                Some(lim) => Some(Self::limit_connections(lim, &mut request.request).await?),
                _ => None,
            };

            let mut resp = client.execute(request.request).await.map_err(map_send_error)?;

            request
                .extensions
                .map(|ext| ext.into_response(resp.extensions_mut()))
                .transpose()?;

            BaseResponse::initialize(resp, permit, request.body_consume_config, Some(runtime)).await
        };

        let fut = spawner.runtime.spawn_handled(fut, cancel);

        let response = tokio::select! {
            res = fut => res?,
            _ = spawner.close_cancellation.cancelled() => Err(ClientClosedError::from_causes("Client was closed", vec![]),)
        }?;

        if request.error_for_status {
            response.error_for_status()?;
        }
        Ok(response)
    }

    pub async fn spawn_reqwest(request: SpawnRequestData, cancel: CancelHandle) -> PyResult<Py<Response>> {
        let resp = Self::spawn_reqwest_inner(request, cancel).await;
        Python::attach(|py| Response::new_py(py, resp?))
    }

    pub fn blocking_spawn_reqwest(request: SpawnRequestData) -> PyResult<Py<BlockingResponse>> {
        let rt = &request.spawner.runtime.clone();
        let resp = rt.blocking_spawn(Self::spawn_reqwest_inner(request, CancelHandle::new()))?;
        Python::attach(|py| BlockingResponse::new_py(py, resp))
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

pub struct SpawnRequestData {
    pub spawner: Spawner,
    pub request: reqwest::Request,
    pub extensions: Option<Extensions>,
    pub error_for_status: bool,
    pub body_consume_config: BodyConsumeConfig,
}
