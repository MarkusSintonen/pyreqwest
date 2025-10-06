use crate::client::RuntimeHandle;
use crate::exceptions::utils::map_read_error;
use bytes::{Bytes, BytesMut};
use futures_util::FutureExt;
use http_body_util::BodyExt;
use pyo3::PyResult;
use pyo3::coroutine::CancelHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::exceptions::asyncio::CancelledError;
use std::collections::VecDeque;
use tokio::sync::OwnedSemaphorePermit;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_READ_BUFFER_LIMIT: usize = 65536;

pub struct BodyReader {
    body_receiver: Option<Receiver>,
    chunks: VecDeque<Bytes>,
    fully_consumed_body: Option<Bytes>,
    content_length: Option<usize>,
    read_bytes: usize,
    runtime: RuntimeHandle,
}
impl BodyReader {
    pub async fn initialize(
        mut response: reqwest::Response,
        mut request_semaphore_permit: Option<OwnedSemaphorePermit>,
        read_config: BodyConsumeConfig,
        runtime: RuntimeHandle,
    ) -> PyResult<(Self, http::response::Parts)> {
        let buffer_limit = match read_config {
            BodyConsumeConfig::FullyConsumed => None,
            BodyConsumeConfig::Streamed(cfg) => Some(cfg.read_buffer_limit),
        };

        let (init_chunks, has_more) = Self::read_limit(&mut response, buffer_limit).await?;
        let (head, body) = Self::response_parts(response);

        let mut body_receiver: Option<Receiver> = None;
        if let Some(buffer_limit) = buffer_limit {
            if has_more {
                body_receiver =
                    Some(Reader::start(body, request_semaphore_permit.take(), buffer_limit, runtime.clone()));
            }
        } else {
            assert!(!has_more, "Should have fully consumed the response");
        }
        if body_receiver.is_none()
            && let Some(a) = request_semaphore_permit.take()
        {
            drop(a)
        } // No more body to read, release the semaphore permit

        let body_reader = BodyReader {
            body_receiver,
            chunks: init_chunks,
            fully_consumed_body: None,
            content_length: Self::content_length(&head.headers),
            read_bytes: 0,
            runtime,
        };
        Ok((body_reader, head))
    }

    pub async fn next_chunk(&mut self, cancel: &mut CancelHandle) -> PyResult<Option<Bytes>> {
        async fn next(this: &mut BodyReader, cancel: &mut CancelHandle) -> PyResult<Option<Bytes>> {
            if let Some(chunk) = this.chunks.pop_front() {
                return Ok(Some(chunk));
            }

            let Some(body_rx) = this.body_receiver.as_mut() else {
                return Ok(None); // No body receiver, fully consumed
            };

            let chunks_buffer = tokio::select! {
                res = body_rx.recv() => res,
                _ = cancel.cancelled().fuse() => Err(CancelledError::new_err("Read was cancelled")),
            };
            let Some(chunks_buffer) = chunks_buffer? else {
                return Ok(None); // No more data
            };

            let mut buffer_iter = chunks_buffer.into_iter();
            let first_chunk = buffer_iter.next();
            for rest_chunk in buffer_iter {
                this.chunks.push_back(rest_chunk);
            }
            Ok(first_chunk)
        }

        match next(self, cancel).await {
            Ok(Some(chunk)) => {
                self.read_bytes += chunk.len();
                Ok(Some(chunk))
            }
            Ok(None) => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub async fn bytes(&mut self, cancel: &mut CancelHandle) -> PyResult<Bytes> {
        if let Some(fully_consumed_body) = self.fully_consumed_body.as_ref() {
            return Ok(fully_consumed_body.clone()); // Zero-copy clone
        }

        if self.read_bytes > 0 {
            return Err(PyRuntimeError::new_err("Response body already consumed"));
        }

        let mut bytes = match self.content_length {
            Some(len) => BytesMut::with_capacity(len),
            None => BytesMut::new(),
        };

        while let Some(chunk) = self.next_chunk(cancel).await? {
            bytes.extend_from_slice(&chunk);
        }

        let bytes = bytes.freeze();
        self.fully_consumed_body = Some(bytes.clone()); // Zero-copy clone
        Ok(bytes)
    }

    pub async fn read(&mut self, amount: usize, cancel: &mut CancelHandle) -> PyResult<Option<Bytes>> {
        if amount == 0 {
            return Ok(Some(Bytes::new()));
        }

        let remaining = self
            .content_length
            .map(|content_len| content_len.saturating_sub(self.read_bytes));
        let capacity = remaining.map(|remaining| remaining.min(amount)).unwrap_or(amount);

        let mut collected = BytesMut::with_capacity(capacity);
        let mut remaining = amount;

        while remaining > 0 {
            if let Some(mut chunk) = self.next_chunk(cancel).await? {
                if chunk.len() > remaining {
                    let extra = chunk.split_off(remaining);
                    self.chunks.push_front(extra);
                }
                collected.extend_from_slice(&chunk);
                remaining -= chunk.len();
            } else {
                break; // No more data
            }
        }

        if collected.is_empty() {
            return Ok(None);
        }
        Ok(Some(collected.freeze()))
    }

    pub fn close(&self) {
        if let Some(body_rx) = self.body_receiver.as_ref() {
            body_rx.close();
        }
    }

    pub fn runtime(&self) -> &RuntimeHandle {
        &self.runtime
    }

    fn response_parts(response: reqwest::Response) -> (http::response::Parts, reqwest::Body) {
        let resp: http::Response<reqwest::Body> = response.into();
        resp.into_parts()
    }

    async fn read_limit(
        response: &mut reqwest::Response,
        byte_limit: Option<usize>,
    ) -> PyResult<(VecDeque<Bytes>, bool)> {
        if byte_limit == Some(0) {
            return Ok((VecDeque::new(), true));
        }

        let mut init_chunks: VecDeque<Bytes> = VecDeque::new();
        let mut has_more = true;
        let mut consumed_bytes = 0;

        while has_more {
            if let Some(chunk) = response.chunk().await.map_err(map_read_error)? {
                consumed_bytes += chunk.len();
                init_chunks.push_back(chunk);

                if let Some(byte_limit) = byte_limit
                    && consumed_bytes >= byte_limit
                {
                    break;
                }
            } else {
                has_more = false;
            }
        }
        Ok((init_chunks, has_more))
    }

    fn content_length(headers: &http::HeaderMap) -> Option<usize> {
        headers.get(http::header::CONTENT_LENGTH)?.to_str().ok()?.parse().ok()
    }
}

struct Receiver {
    rx: tokio::sync::mpsc::Receiver<PyResult<Vec<Bytes>>>,
    close_token: CancellationToken,
}
impl Receiver {
    async fn recv(&mut self) -> PyResult<Option<Vec<Bytes>>> {
        self.rx.recv().await.transpose()
    }

    fn close(&self) {
        self.close_token.cancel();
    }
}

struct Reader {
    buffer: Option<Vec<Bytes>>,
    tot_bytes: usize,
    buffer_size: usize,
    tx: tokio::sync::mpsc::Sender<PyResult<Vec<Bytes>>>,
}

impl Reader {
    fn start(
        mut body: reqwest::Body,
        mut request_semaphore_permit: Option<OwnedSemaphorePermit>,
        buffer_size: usize,
        runtime: RuntimeHandle,
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

        let _ = runtime.spawn(async move {
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
                            if let Ok(chunk) = frame.into_data()
                                && !chunk.is_empty()
                                && !reader.send_chunk(chunk).await
                            {
                                break; // Receiver was dropped :NOCOV:
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
            return false; // Already finalized :NOCOV:
        };

        self.tot_bytes += chunk.len();
        buffer.push(chunk);

        if self.tot_bytes < self.buffer_size {
            return true;
        }

        let new_buffer = Vec::with_capacity(buffer.capacity()); // Start new chunks buffer
        self.tot_bytes = 0;

        self.tx.send(Ok(std::mem::replace(buffer, new_buffer))).await.is_ok()
    }

    async fn finalize(&mut self) {
        if let Some(buffer) = self.buffer.take()
            && !buffer.is_empty()
        {
            _ = self.tx.send(Ok(buffer)).await;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BodyConsumeConfig {
    FullyConsumed,
    Streamed(StreamedReadConfig),
}

#[derive(Debug, Clone, Copy)]
pub struct StreamedReadConfig {
    pub read_buffer_limit: usize,
}
impl Default for StreamedReadConfig {
    fn default() -> Self {
        Self {
            read_buffer_limit: DEFAULT_READ_BUFFER_LIMIT,
        }
    }
}
