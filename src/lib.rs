// rustimport:pyo3

mod asyncio;
mod client;
mod client_builder;
mod exceptions;
mod http_types;
mod middleware;
mod multipart_form;
mod proxy_config;
mod request;
mod request_body;
mod request_builder;
mod request_wrapper;
mod response;
mod runtime;
mod utils;

use crate::client::Client;
use crate::exceptions::{
    PoolTimeoutError, ReadBodyError, ReadError, ReadTimeoutError, RequestError, SendBodyError, SendConnectionError,
    SendError, SendTimeoutError,
};
use crate::middleware::Next;
use crate::proxy_config::ProxyConfig;
use crate::request::Request;
use crate::request_builder::RequestBuilder;
use crate::response::Response;
use pyo3::prelude::*;

#[pymodule]
#[pyo3(name = "_core")]
fn pyreqwest(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Client>()?;
    module.add_class::<RequestBuilder>()?;
    module.add_class::<Request>()?;
    module.add_class::<Response>()?;
    module.add_class::<Next>()?;
    module.add_class::<ProxyConfig>()?;

    module.add("RequestError", module.py().get_type::<RequestError>())?;
    module.add("SendError", module.py().get_type::<SendError>())?;
    module.add("SendConnectionError", module.py().get_type::<SendConnectionError>())?;
    module.add("SendBodyError", module.py().get_type::<SendBodyError>())?;
    module.add("SendTimeoutError", module.py().get_type::<SendTimeoutError>())?;
    module.add("PoolTimeoutError", module.py().get_type::<PoolTimeoutError>())?;
    module.add("ReadError", module.py().get_type::<ReadError>())?;
    module.add("ReadBodyError", module.py().get_type::<ReadBodyError>())?;
    module.add("ReadTimeoutError", module.py().get_type::<ReadTimeoutError>())?;

    Ok(())
}
