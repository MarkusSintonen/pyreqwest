use bytes::Bytes;
use pyo3::PyResult;

pub struct Sender {
    buffer: Option<Vec<Bytes>>,
    tot_bytes: usize,
    buffer_size: usize,
    tx: tokio::sync::mpsc::Sender<PyResult<Vec<Bytes>>>,
}

pub struct Receiver {
    rx: tokio::sync::mpsc::Receiver<PyResult<Vec<Bytes>>>,
}

pub fn bytes_channel(buffer_size: usize) -> (Sender, Receiver) {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    (Sender { buffer: Some(Vec::new()), tot_bytes: 0, buffer_size, tx }, Receiver { rx })
}

impl Sender {
    pub async fn send(&mut self, chunk_res: PyResult<Bytes>) -> bool {
        if self.tx.is_closed() {
            return false
        }

        let Some(buffer) = self.buffer.as_mut() else {
            return false // Already finalized
        };

        match chunk_res {
            Err(err) => self.tx.send(Err(err)).await.is_ok(),
            Ok(chunk) => {
                self.tot_bytes += chunk.len();
                buffer.push(chunk);

                if self.tot_bytes < self.buffer_size {
                    return true
                }

                self.tot_bytes = 0;
                self.tx.send(Ok(buffer.drain(..).collect())).await.is_ok()
            }
        }
    }

    pub async fn finalize(&mut self) {
        let Some(buffer) = self.buffer.take() else {
            return // Already finalized
        };
        if buffer.is_empty() {
            return
        }
        if self.tx.is_closed() {
            return // Closed
        }
        _ = self.tx.send(Ok(buffer)).await;
    }
}

impl Receiver {
    pub async fn recv(&mut self) -> PyResult<Option<Vec<Bytes>>> {
        self.rx.recv().await.transpose()
    }

    pub fn close(&mut self) {
        self.rx.close();
    }
}
