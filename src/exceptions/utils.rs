use crate::exceptions::exceptions::{
    BuilderError, ConnectError, ConnectTimeoutError, DecodeError, ReadError, ReadTimeoutError, RedirectError,
    RequestError, StatusError, WriteError, WriteTimeoutError,
};
use pyo3::PyErr;
use serde_json::json;

pub fn map_send_error(e: reqwest::Error) -> PyErr {
    inner_map_io_error(e, ErrorKind::Send)
}

pub fn map_read_error(e: reqwest::Error) -> PyErr {
    inner_map_io_error(e, ErrorKind::Read)
}

fn inner_map_io_error(e: reqwest::Error, kind: ErrorKind) -> PyErr {
    if e.is_timeout() {
        if e.is_body() {
            match kind {
                ErrorKind::Send => WriteTimeoutError::from_err("request body timeout", &e),
                ErrorKind::Read => ReadTimeoutError::from_err("response body timeout", &e),
            }
        } else {
            ConnectTimeoutError::from_err("connection timeout", &e)
        }
    } else if e.is_connect() {
        if e.is_body() {
            match kind {
                ErrorKind::Send => WriteError::from_err("request body connection error", &e),
                ErrorKind::Read => ReadError::from_err("response body connection error", &e),
            }
        } else {
            ConnectError::from_err("connection error", &e)
        }
    } else if e.is_decode() {
        DecodeError::from_err("error decoding response body", &e)
    } else if e.is_redirect() {
        RedirectError::from_err("error following redirect", &e)
    } else if e.is_builder() {
        BuilderError::from_err("builder error", &e)
    } else if e.is_status() {
        StatusError::new_err(&e.to_string(), Some(json!({"status": e.status().unwrap().as_u16()})))
    } else if e.is_body() {
        match kind {
            ErrorKind::Send => RequestError::from_err("request body error", &e),
            ErrorKind::Read => RequestError::from_err("response body error", &e),
        }
    } else {
        RequestError::from_err("error sending request", &e)
    }
}

enum ErrorKind {
    Send,
    Read,
}
