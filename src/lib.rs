mod asyncio;
mod client;
mod exceptions;
mod http;
mod middleware;
mod multipart;
mod proxy;
mod request;
mod response;

use crate::client::Client;
use crate::client::ClientBuilder;
use crate::exceptions::{
    PoolTimeoutError, ReadBodyError, ReadError, ReadTimeoutError, RequestError, SendBodyError, SendConnectionError,
    SendError, SendTimeoutError,
};
use crate::http::url::Url;
use crate::middleware::Next;
use crate::proxy::Proxy;
use crate::request::Request;
use crate::request::RequestBody;
use crate::request::RequestBuilder;
use crate::response::Response;
use pyo3::prelude::*;

#[pymodule(name = "_pyreqwest")]
fn pyreqwest(py: Python, module: &Bound<'_, PyModule>) -> PyResult<()> {
    let sub = PyModule::new(py, "client")?;
    sub.add_class::<ClientBuilder>()?;
    sub.add_class::<Client>()?;
    module.add_submodule(&sub)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pyreqwest._pyreqwest.client", sub)?;

    let sub = PyModule::new(py, "request")?;
    sub.add_class::<RequestBuilder>()?;
    sub.add_class::<Request>()?;
    sub.add_class::<RequestBody>()?;
    module.add_submodule(&sub)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pyreqwest._pyreqwest.request", sub)?;

    let sub = PyModule::new(py, "response")?;
    sub.add_class::<Response>()?;
    module.add_submodule(&sub)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pyreqwest._pyreqwest.response", sub)?;

    let sub = PyModule::new(py, "middleware")?;
    sub.add_class::<Next>()?;
    module.add_submodule(&sub)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pyreqwest._pyreqwest.middleware", sub)?;

    let sub = PyModule::new(py, "proxy")?;
    sub.add_class::<Proxy>()?;
    module.add_submodule(&sub)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pyreqwest._pyreqwest.proxy", sub)?;

    let sub = PyModule::new(py, "multipart")?;
    sub.add_class::<multipart::Form>()?;
    module.add_submodule(&sub)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pyreqwest._pyreqwest.multipart", sub)?;

    let sub = PyModule::new(py, "http")?;
    sub.add_class::<Url>()?;
    module.add_submodule(&sub)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pyreqwest._pyreqwest.http", sub)?;

    let sub = PyModule::new(py, "exceptions")?;
    sub.add("RequestError", py.get_type::<RequestError>())?;
    sub.add("SendError", py.get_type::<SendError>())?;
    sub.add("SendConnectionError", py.get_type::<SendConnectionError>())?;
    sub.add("SendBodyError", py.get_type::<SendBodyError>())?;
    sub.add("SendTimeoutError", py.get_type::<SendTimeoutError>())?;
    sub.add("PoolTimeoutError", py.get_type::<PoolTimeoutError>())?;
    sub.add("ReadError", py.get_type::<ReadError>())?;
    sub.add("ReadBodyError", py.get_type::<ReadBodyError>())?;
    sub.add("ReadTimeoutError", py.get_type::<ReadTimeoutError>())?;
    module.add_submodule(&sub)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pyreqwest._pyreqwest.exceptions", sub)?;

    Ok(())
}
