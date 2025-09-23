use crate::exceptions::BodyError;
use crate::exceptions::exceptions::{
    BuilderError, ConnectError, ConnectTimeoutError, DecodeError, ReadError, ReadTimeoutError, RedirectError,
    RequestError, StatusError, WriteError, WriteTimeoutError,
};
use pyo3::{PyErr, Python};
use serde_json::json;
use std::error::Error;
use std::io;

pub fn map_send_error(e: reqwest::Error) -> PyErr {
    inner_map_io_error(e, ErrorKind::Send)
}

pub fn map_read_error(e: reqwest::Error) -> PyErr {
    inner_map_io_error(e, ErrorKind::Read)
}

fn inner_map_io_error(e: reqwest::Error, kind: ErrorKind) -> PyErr {
    if let Some(py_err) = inner_py_err(&e) {
        return py_err;
    }
    let causes = error_causes_iter(&e).collect::<Vec<_>>();
    if e.is_timeout() {
        if is_body_error(&e) {
            match kind {
                ErrorKind::Send => WriteTimeoutError::from_causes("request body timeout", causes),
                ErrorKind::Read => ReadTimeoutError::from_causes("response body timeout", causes),
            }
        } else {
            ConnectTimeoutError::from_causes("connection timeout", causes)
        }
    } else if is_connect_error(&e) {
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
    } else if is_body_error(&e) {
        match kind {
            ErrorKind::Send => BodyError::from_causes("request body error", causes),
            ErrorKind::Read => BodyError::from_causes("response body error", causes),
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

pub fn error_causes_iter<'a>(err: &'a (dyn Error + 'static)) -> impl Iterator<Item = &'a (dyn Error + 'static)> {
    let mut next = Some(err);
    std::iter::from_fn(move || {
        let res = next;
        next = next.and_then(|e| e.source());
        res
    })
}

fn inner_py_err(err: &(dyn Error + 'static)) -> Option<PyErr> {
    for e in error_causes_iter(err) {
        if let Some(py_err) = e.downcast_ref::<PyErr>() {
            return Some(Python::attach(|py| py_err.clone_ref(py)));
        }
    }
    None
}

fn is_connect_error(err: &reqwest::Error) -> bool {
    if err.is_connect() {
        return true;
    }
    for e in error_causes_iter(err) {
        if e.downcast_ref::<hyper::Error>()
            .is_some_and(|e| e.is_incomplete_message())
        {
            return true;
        }
        if let Some(io_err) = e.downcast_ref::<io::Error>()
            && is_io_error_connection_error(io_err)
        {
            return true;
        }
        if e.to_string().contains("connection error") {
            return true;
        }
    }
    false
}

fn is_body_error(err: &reqwest::Error) -> bool {
    for e in error_causes_iter(err) {
        if e.downcast_ref::<reqwest::Error>().is_some_and(|e| e.is_body()) {
            return true;
        }
    }
    false
}

fn is_io_error_connection_error(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::HostUnreachable
            | io::ErrorKind::NetworkUnreachable
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::NotConnected
            | io::ErrorKind::AddrInUse
            | io::ErrorKind::AddrNotAvailable
            | io::ErrorKind::NetworkDown
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::TimedOut
            | io::ErrorKind::UnexpectedEof
    )
}
