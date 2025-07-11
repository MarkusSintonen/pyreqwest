use crate::exceptions::{
    ReadBodyError, ReadError, ReadTimeoutError, SendBodyError, SendConnectionError, SendError, SendTimeoutError,
};
use pyo3::PyErr;
use std::error::Error;

pub fn map_send_error(e: reqwest::Error) -> PyErr {
    if e.is_connect() {
        SendConnectionError::new_err(format!("Connection error on send: {}", fmt_error(&e)))
    } else if e.is_timeout() {
        SendTimeoutError::new_err(format!("Timeout on send: {}", fmt_error(&e)))
    } else if e.is_body() {
        SendBodyError::new_err(format!("Error on sending body: {}", fmt_error(&e)))
    } else {
        SendError::new_err(format!("Error on send: {}", fmt_error(&e)))
    }
}

pub fn map_read_error(e: reqwest::Error) -> PyErr {
    if e.is_body() {
        ReadBodyError::new_err(format!("Error on reading body: {}", fmt_error(&e)))
    } else if e.is_timeout() {
        ReadTimeoutError::new_err(format!("Timeout on reading body: {}", fmt_error(&e)))
    } else {
        ReadError::new_err(format!("Error on reading body: {}", fmt_error(&e)))
    }
}

fn fmt_error<E: Error>(error: &E) -> String {
    let mut message = error.to_string();
    if let Some(source) = error.source() {
        message.push_str(&format!(" ({})", source));
    }
    message
}
