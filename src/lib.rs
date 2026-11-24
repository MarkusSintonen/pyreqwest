mod client;
mod cookie;
mod exceptions;
mod http;
mod internal;
mod logging;
mod middleware;
mod multipart;
mod proxy;
mod request;
mod response;
mod runtime;

use pyo3::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::internal::module_utils::{register_collections_abc, register_submodule};
use crate::internal::types::Method;
use crate::request::{OneOffRequestBuilder, SyncOneOffRequestBuilder};

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[pymodule(name = "_pyreqwest", gil_used = false)]
mod pyreqwest {
    use super::*;

    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
        let start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        module.add("__version__", env!("CARGO_PKG_VERSION"))?;
        module.add("_start_time_ns", start)
    }

    #[pymodule]
    mod client {
        use super::*;
        #[pymodule_export]
        use crate::client::{BaseClient, BaseClientBuilder, Client, ClientBuilder, SyncClient, SyncClientBuilder};
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
            BaseRequestBuilder, ConsumedRequest, OneOffRequestBuilder, Request, RequestBody, RequestBuilder,
            StreamRequest, SyncConsumedRequest, SyncOneOffRequestBuilder, SyncRequestBuilder, SyncStreamRequest,
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

    #[pymodule]
    mod simple {
        use super::*;
        #[pymodule]
        mod request {
            use super::*;
            impl_oneoff_functions!(OneOffRequestBuilder);
            #[pymodule_init]
            fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
                register_oneoff_functions!(module);
                register_submodule(module, "simple.request")
            }
        }

        #[pymodule]
        mod sync_request {
            use super::*;
            impl_oneoff_functions!(SyncOneOffRequestBuilder);
            #[pymodule_init]
            fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
                register_oneoff_functions!(module);
                register_submodule(module, "simple.sync_request")
            }
        }
    }

    #[pymodule]
    mod logging {
        use super::*;
        #[pymodule_export]
        use crate::logging::logger::flush_logs;
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_submodule(module, "logging")
        }
    }

    #[pymodule]
    mod runtime {
        use super::*;
        #[pymodule_export]
        use crate::runtime::{
            runtime_blocking_thread_keep_alive, runtime_max_blocking_threads, runtime_multithreaded_default,
            runtime_worker_threads,
        };
        #[pymodule_init]
        fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
            register_submodule(module, "runtime")
        }
    }
}
