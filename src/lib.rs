mod asyncio;
mod client;
mod exceptions;
mod http_types;
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
use crate::middleware::Next;
use crate::proxy::Proxy;
use crate::request::Request;
use crate::request::RequestBody;
use crate::request::RequestBuilder;
use crate::response::Response;
use pyo3::prelude::*;

#[pymodule]
fn pyreqwest(py: Python, module: &Bound<'_, PyModule>) -> PyResult<()> {
    let client = PyModule::new(py, "client")?;
    client.add_class::<ClientBuilder>()?;
    client.add_class::<Client>()?;
    module.add_submodule(&client)?;

    let request = PyModule::new(py, "request")?;
    request.add_class::<RequestBuilder>()?;
    request.add_class::<Request>()?;
    request.add_class::<RequestBody>()?;
    module.add_submodule(&request)?;

    let response = PyModule::new(py, "response")?;
    response.add_class::<Response>()?;
    module.add_submodule(&response)?;

    let middleware = PyModule::new(py, "middleware")?;
    middleware.add_class::<Next>()?;
    module.add_submodule(&middleware)?;

    let proxy = PyModule::new(py, "proxy")?;
    proxy.add_class::<Proxy>()?;
    module.add_submodule(&proxy)?;

    let multipart = PyModule::new(py, "multipart")?;
    multipart.add_class::<multipart::Form>()?;
    module.add_submodule(&multipart)?;

    let exceptions = PyModule::new(py, "exceptions")?;
    exceptions.add("RequestError", py.get_type::<RequestError>())?;
    exceptions.add("SendError", py.get_type::<SendError>())?;
    exceptions.add("SendConnectionError", py.get_type::<SendConnectionError>())?;
    exceptions.add("SendBodyError", py.get_type::<SendBodyError>())?;
    exceptions.add("SendTimeoutError", py.get_type::<SendTimeoutError>())?;
    exceptions.add("PoolTimeoutError", py.get_type::<PoolTimeoutError>())?;
    exceptions.add("ReadError", py.get_type::<ReadError>())?;
    exceptions.add("ReadBodyError", py.get_type::<ReadBodyError>())?;
    exceptions.add("ReadTimeoutError", py.get_type::<ReadTimeoutError>())?;
    module.add_submodule(&exceptions)?;

    Ok(())
}
