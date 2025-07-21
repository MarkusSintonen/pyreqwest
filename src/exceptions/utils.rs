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
    if e.is_timeout() {
        if is_body_error(&e) {
            match kind {
                ErrorKind::Send => WriteTimeoutError::from_err("request body timeout", &e),
                ErrorKind::Read => ReadTimeoutError::from_err("response body timeout", &e),
            }
        } else {
            ConnectTimeoutError::from_err("connection timeout", &e)
        }
    } else if e.is_connect() {
        if is_body_error(&e) {
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

pub fn sources<'a, E: Error>(err: &'a E) -> Vec<&'a (dyn Error + 'static)> {
    let mut causes = Vec::new();
    let mut cur = err.source();
    while let Some(source) = cur {
        causes.push(source);
        cur = source.source();
    }
    causes
}
