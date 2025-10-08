use crate::exceptions::BodyDecodeError;
use crate::exceptions::exceptions::{
    BuilderError, ConnectError, ConnectTimeoutError, DecodeError, ReadError, ReadTimeoutError, RedirectError,
    RequestError, WriteError, WriteTimeoutError,
};
use pyo3::{PyErr, Python};
use regex::RegexSet;
use std::error::Error;
use std::sync::LazyLock;

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
    if is_timeout_error(&e) {
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
    } else if is_decode_error(&e) {
        if is_body_error(&e) {
            BodyDecodeError::from_causes("error decoding body", causes)
        } else {
            DecodeError::from_causes("error decoding response", causes)
        }
    } else if e.is_redirect() {
        RedirectError::from_causes("error following redirect", causes)
    } else if e.is_builder() {
        BuilderError::from_causes("builder error", causes)
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

fn is_timeout_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || error_causes_matches(err, &TIMEOUT_ERROR_PATTERN)
}

fn is_connect_error(err: &reqwest::Error) -> bool {
    err.is_connect() || error_causes_matches(err, &CONNECTION_ERROR_PATTERN)
}

fn is_decode_error(err: &reqwest::Error) -> bool {
    for e in error_causes_iter(err) {
        if e.downcast_ref::<reqwest::Error>().is_some_and(|e| e.is_decode())
            || e.downcast_ref::<hyper::Error>().is_some_and(|e| e.is_parse())
        {
            return true;
        }
    }
    false
}

fn is_body_error(err: &reqwest::Error) -> bool {
    for e in error_causes_iter(err) {
        if e.downcast_ref::<reqwest::Error>()
            .is_some_and(|e| e.is_body() || e.is_decode())
        {
            return true;
        }
    }
    false
}

fn error_causes_matches(err: &reqwest::Error, pattern: &RegexSet) -> bool {
    error_causes_iter(err).any(|e| pattern.is_match(&e.to_string()))
}

// Reqwest does not provide a good way to exhaustively check for connection errors.
// Its "err.is_connect" check is not good enough. Neither are hypers checks.
static CONNECTION_ERROR_PATTERN: LazyLock<RegexSet> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    RegexSet::new([
        r"(?i)connection error",
        r"(?i)connection(.*) closed",
        r"(?i)connection refused",
        r"(?i)connection reset",
        r"(?i)connection aborted",
        r"(?i)host unreachable",
        r"(?i)network unreachable",
        r"(?i)not connected",
        r"(?i)address in use",
        r"(?i)address not available",
        r"(?i)network down",
        r"(?i)broken pipe",
        r"(?i)unexpected end of file",
        r"(?i)unexpected eof",
    ])
    .expect("invalid connection error regex")
});

static TIMEOUT_ERROR_PATTERN: LazyLock<RegexSet> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    RegexSet::new([r"(?i)timed out", r"(?i)timeout"]).expect("invalid timeout error regex")
});
