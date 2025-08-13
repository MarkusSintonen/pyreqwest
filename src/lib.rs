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
use pyo3::types::PyType;
use pyo3::{PyTypeInfo, intern};

#[pymodule(name = "_pyreqwest")]
mod pyreqwest {
    use super::*;

    #[pymodule]
    mod client {
        use super::*;
        #[pymodule_export]
        use crate::client::{Client, ClientBuilder, Runtime};
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_module_hack(module, "client")
        }
    }

    #[pymodule]
    mod request {
        use super::*;
        #[pymodule_export]
        use crate::request::{ConsumedRequest, Request, RequestBuilder, StreamRequest};
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_module_hack(module, "request")
        }
    }

    #[pymodule]
    mod response {
        use super::*;
        #[pymodule_export]
        use crate::response::{Response, ResponseBuilder};
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_module_hack(module, "response")
        }
    }

    #[pymodule]
    mod middleware {
        use super::*;
        #[pymodule_export]
        use crate::middleware::Next;
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_module_hack(module, "middleware")
        }
    }

    #[pymodule]
    mod proxy {
        use super::*;
        #[pymodule_export]
        use crate::proxy::Proxy;
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_module_hack(module, "proxy")
        }
    }

    #[pymodule]
    mod multipart {
        use super::*;
        #[pymodule_export]
        use crate::multipart::Form;
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_module_hack(module, "multipart")
        }
    }

    #[pymodule]
    mod http {
        use super::*;
        #[pymodule_export]
        use crate::http::{Body, HeaderMap, HeaderMapItemsView, HeaderMapKeysView, HeaderMapValuesView, Mime, Url};
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_collections_abc::<HeaderMap>(module.py(), "MutableMapping")?;
            register_collections_abc::<HeaderMapItemsView>(module.py(), "ItemsView")?;
            register_collections_abc::<HeaderMapKeysView>(module.py(), "KeysView")?;
            register_collections_abc::<HeaderMapValuesView>(module.py(), "ValuesView")?;
            register_module_hack(module, "http")
        }
    }
}

fn register_collections_abc<T: PyTypeInfo>(py: Python, base: &str) -> PyResult<()> {
    py.import("collections")?
        .getattr("abc")?
        .getattr(base)?
        .call_method1(intern!(py, "register"), (PyType::new::<T>(py),))
        .map(|_| ())
}

// https://github.com/PyO3/pyo3/issues/759
fn register_module_hack(module: &Bound<'_, PyModule>, name: &str) -> PyResult<()> {
    module
        .py()
        .import("sys")?
        .getattr("modules")?
        .set_item(format!("pyreqwest._pyreqwest.{}", name), module)
}
