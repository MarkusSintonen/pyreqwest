use crate::exceptions::exceptions::{
    BuilderError, ConnectError, ConnectTimeoutError, DecodeError, ReadError, ReadTimeoutError, RedirectError,
    RequestError, StatusError, WriteError, WriteTimeoutError,
};
use pyo3::PyErr;
use serde_json::json;
use std::error::Error;

pub fn map_send_error(e: reqwest::Error) -> PyErr {
    inner_map_io_error(e, ErrorKind::Send)
}

pub fn map_read_error(e: reqwest::Error) -> PyErr {
    inner_map_io_error(e, ErrorKind::Read)
}

fn inner_map_io_error(e: reqwest::Error, kind: ErrorKind) -> PyErr {
    let causes = sources(&e);
    if e.is_timeout() {
        if is_body_error(&e) {
            match kind {
                ErrorKind::Send => WriteTimeoutError::from_causes("request body timeout", causes),
                ErrorKind::Read => ReadTimeoutError::from_causes("response body timeout", causes),
            }
        } else {
            ConnectTimeoutError::from_causes("connection timeout", causes)
        }
    } else if e.is_connect() {
        if is_body_error(&e) {
            match kind {
                ErrorKind::Send => WriteError::from_causes("request body connection error", causes),
                ErrorKind::Read => ReadError::from_causes("response body connection error", causes),
            }
        } else {
            ConnectError::from_causes("connection error", causes)
        }
    } else if e.is_decode() {
        DecodeError::from_causes("error decoding response body", causes)
    } else if e.is_redirect() {
        RedirectError::from_causes("error following redirect", causes)
    } else if e.is_builder() {
        BuilderError::from_causes("builder error", causes)
    } else if e.is_status() {
        StatusError::from_custom(&e.to_string(), json!({"status": e.status().unwrap().as_u16()}))
    } else if e.is_body() {
        match kind {
            ErrorKind::Send => RequestError::from_causes("request body error", causes),
            ErrorKind::Read => RequestError::from_causes("response body error", causes),
        }
    } else {
        RequestError::from_causes("error sending request", causes)
    }
}

#[derive(PartialEq, Debug)]
enum ErrorKind {
    Send,
    Read,
}

pub fn is_body_error<E: Error>(err: &E) -> bool {
    sources(err)
        .iter()
        .any(|src| src.downcast_ref::<reqwest::Error>().map_or(false, |e| e.is_body()))
}

pub fn sources<'a>(err: &'a dyn Error) -> Vec<&'a (dyn Error + 'static)> {
    let mut causes = Vec::new();
    let mut cur = err.source();
    while let Some(source) = cur {
        causes.push(source);
        cur = source.source();
    }
    causes
}
