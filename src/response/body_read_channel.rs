use crate::client::Handle;
use crate::exceptions::utils::map_read_error;
use bytes::Bytes;
use http_body_util::BodyExt;
use pyo3::PyResult;
use tokio::sync::OwnedSemaphorePermit;
use tokio_util::sync::CancellationToken;

pub struct Receiver {
    rx: tokio::sync::mpsc::Receiver<PyResult<Vec<Bytes>>>,
    close_token: CancellationToken,
}
impl Receiver {
    pub async fn recv(&mut self) -> PyResult<Option<Vec<Bytes>>> {
        self.rx.recv().await.transpose()
    }

    pub fn close(&self) {
        self.close_token.cancel();
    }
}

struct Reader {
    buffer: Option<Vec<Bytes>>,
    tot_bytes: usize,
    buffer_size: usize,
    tx: tokio::sync::mpsc::Sender<PyResult<Vec<Bytes>>>,
}

pub fn body_read_channel(
    body: reqwest::Body,
    request_semaphore_permit: Option<OwnedSemaphorePermit>,
    buffer_size: usize,
    runtime: Option<Handle>,
) -> Receiver {
    Reader::start(body, request_semaphore_permit, buffer_size, runtime)
}

impl Reader {
    fn start(
        mut body: reqwest::Body,
        mut request_semaphore_permit: Option<OwnedSemaphorePermit>,
        buffer_size: usize,
        runtime: Option<Handle>,
    ) -> Receiver {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let close_token = CancellationToken::new();
        let close_token_child = close_token.child_token();

        let mut reader = Reader {
            buffer: Some(Vec::new()),
            tot_bytes: 0,
            buffer_size,
            tx,
        };
        let runtime = runtime.unwrap_or_else(Handle::current);

        runtime.0.spawn(async move {
            let fut = async move {
                loop {
                    match body.frame().await.transpose().map_err(map_read_error) {
                        Err(err) => {
                            let _ = reader.tx.send(Err(err)).await;
                            break; // Stop on error
                        }
                        Ok(None) => {
                            reader.finalize().await;
                            break; // All was consumed
                        }
                        Ok(Some(frame)) => {
                            if let Ok(chunk) = frame.into_data() {
                                if !reader.send_chunk(chunk).await {
                                    break; // Receiver was dropped
                                }
                            }
                        }
                    }
                }
            };

            tokio::select! {
                _ = fut => {},
                _ = close_token_child.cancelled() => {}
            }

            _ = request_semaphore_permit.take();
        });

        Receiver { rx, close_token }
    }

    async fn send_chunk(&mut self, chunk: Bytes) -> bool {
        let Some(buffer) = self.buffer.as_mut() else {
            return false; // Already finalized
        };

        self.tot_bytes += chunk.len();
        buffer.push(chunk);

        if self.tot_bytes < self.buffer_size {
            return true;
        }

        let new_buffer = Vec::with_capacity(buffer.capacity());

        self.tot_bytes = 0;
        self.tx.send(Ok(std::mem::replace(buffer, new_buffer))).await.is_ok()
    }

    pub async fn finalize(&mut self) {
        let Some(buffer) = self.buffer.take() else {
            return; // Already finalized
        };
        if buffer.is_empty() {
            return;
        }
        _ = self.tx.send(Ok(buffer)).await;
    }
}
