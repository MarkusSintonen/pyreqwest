mod asyncio;
mod client;
mod exceptions;
mod http;
mod middleware;
mod multipart;
mod proxy;
mod request;
mod response;

use pyo3::prelude::*;

#[pymodule(name = "_pyreqwest")]
mod pyreqwest {
    use super::*;

    #[pymodule]
    mod client {
        use super::*;
        #[pymodule_export]
        use crate::client::{Client, ClientBuilder};
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_hack(module, "client")
        }
    }

    #[pymodule]
    mod request {
        use super::*;
        #[pymodule_export]
        use crate::request::{ConsumedRequest, Request, RequestBuilder, StreamRequest};
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_hack(module, "request")
        }
    }

    #[pymodule]
    mod response {
        use super::*;
        #[pymodule_export]
        use crate::response::{Response, ResponseBuilder};
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_hack(module, "response")
        }
    }

    #[pymodule]
    mod middleware {
        use super::*;
        #[pymodule_export]
        use crate::middleware::Next;
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_hack(module, "middleware")
        }
    }

    #[pymodule]
    mod proxy {
        use super::*;
        #[pymodule_export]
        use crate::proxy::Proxy;
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_hack(module, "proxy")
        }
    }

    #[pymodule]
    mod multipart {
        use super::*;
        #[pymodule_export]
        use crate::multipart::Form;
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_hack(module, "multipart")
        }
    }

    #[pymodule]
    mod http {
        use super::*;
        #[pymodule_export]
        use crate::http::{Body, HeaderMap, Url};
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_hack(module, "http")
        }
    }
}

// https://github.com/PyO3/pyo3/issues/759
fn register_hack(module: &Bound<'_, PyModule>, name: &str) -> PyResult<()> {
    let mod_name = format!("pyreqwest._pyreqwest.{}", name);
    module
        .py()
        .import("sys")?
        .getattr("modules")?
        .set_item(mod_name, module)
}
