mod allow_threads;
mod asyncio;
mod client;
mod cookie;
mod exceptions;
mod http;
mod internal;
mod middleware;
mod multipart;
mod proxy;
mod request;
mod response;

use pyo3::prelude::*;
use pyo3::types::PyType;
use pyo3::{PyTypeInfo, intern};

#[pymodule(name = "_pyreqwest", gil_used = false)]
mod pyreqwest {
    use super::*;

    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
        module.add("__version__", env!("CARGO_PKG_VERSION"))
    }

    #[pymodule]
    mod client {
        use super::*;
        #[pymodule_export]
        use crate::client::{
            BaseClient, BaseClientBuilder, Client, ClientBuilder, Runtime, SyncClient, SyncClientBuilder,
        };
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_submodule(module, "client")
        }
    }

    #[pymodule]
    mod request {
        use super::*;
        #[pymodule_export]
        use crate::request::{
            BaseRequestBuilder, ConsumedRequest, Request, RequestBody, RequestBuilder, StreamRequest,
            SyncConsumedRequest, SyncRequestBuilder, SyncStreamRequest,
        };
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_submodule(module, "request")
        }
    }

    #[pymodule]
    mod response {
        use super::*;
        #[pymodule_export]
        use crate::response::{
            BaseResponse, Response, ResponseBodyReader, ResponseBuilder, SyncResponse, SyncResponseBodyReader,
        };
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_submodule(module, "response")
        }
    }

    #[pymodule]
    mod middleware {
        use super::*;
        #[pymodule_export]
        use crate::middleware::{Next, SyncNext};
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_submodule(module, "middleware")
        }
    }

    #[pymodule]
    mod proxy {
        use super::*;
        #[pymodule_export]
        use crate::proxy::ProxyBuilder;
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_submodule(module, "proxy")
        }
    }

    #[pymodule]
    mod multipart {
        use super::*;
        #[pymodule_export]
        use crate::multipart::{FormBuilder, PartBuilder};
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_submodule(module, "multipart")
        }
    }

    #[pymodule]
    mod http {
        use super::*;
        #[pymodule_export]
        use crate::http::{HeaderMap, HeaderMapItemsView, HeaderMapKeysView, HeaderMapValuesView, Mime, Url};
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_collections_abc::<HeaderMap>(module.py(), "MutableMapping")?;
            register_collections_abc::<HeaderMapItemsView>(module.py(), "ItemsView")?;
            register_collections_abc::<HeaderMapKeysView>(module.py(), "KeysView")?;
            register_collections_abc::<HeaderMapValuesView>(module.py(), "ValuesView")?;
            register_submodule(module, "http")
        }
    }

    #[pymodule]
    mod cookie {
        use super::*;
        #[pymodule_export]
        use crate::cookie::{Cookie, CookieStore};
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_submodule(module, "cookie")
        }
    }

    #[pymodule]
    mod bytes {
        use super::*;
        #[pymodule_export]
        use pyo3_bytes::PyBytes;
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_collections_abc::<PyBytes>(module.py(), "Buffer")?;
            register_submodule(module, "bytes")
        }
    }
}

fn register_collections_abc<T: PyTypeInfo>(py: Python, base: &str) -> PyResult<()> {
    if base == "Buffer" && py.version_info() < (3, 12) {
        return Ok(()); // Buffer ABC was added in Python 3.12
    }

    py.import("collections")?
        .getattr("abc")?
        .getattr(base)?
        .call_method1(intern!(py, "register"), (PyType::new::<T>(py),))
        .map(|_| ())
}

fn register_submodule(module: &Bound<'_, PyModule>, submodule_name: &str) -> PyResult<()> {
    // https://github.com/PyO3/pyo3/issues/759
    module
        .py()
        .import("sys")?
        .getattr("modules")?
        .set_item(format!("pyreqwest._pyreqwest.{}", submodule_name), module)?;

    fix_module(module, Some(submodule_name))
}

fn fix_module(module: &Bound<'_, PyModule>, submodule_name: Option<&str>) -> PyResult<()> {
    // Need to fix module names, otherwise pyo3 uses "builtin" as module name. This breaks doc generation.
    for attr_name in module.dir()?.iter() {
        let attr_name: &str = attr_name.extract()?;
        if attr_name.starts_with("_") {
            continue;
        }
        if let Some(submodule_name) = submodule_name {
            module
                .getattr(attr_name)?
                .setattr("__module__", format!("pyreqwest.{}", submodule_name))?;
        } else {
            module.getattr(attr_name)?.setattr("__module__", "pyreqwest")?;
        }
    }
    Ok(())
}
