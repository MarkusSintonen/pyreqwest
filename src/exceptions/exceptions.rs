use crate::http::JsonValue;
use pyo3::prelude::*;
use pyo3::pyobject_native_type_core;
use pyo3::sync::GILOnceCell;
use pyo3::types::PyType;
use serde_json::json;
use std::error::Error;

macro_rules! define_exception {
    ($name:ident) => {
        #[allow(unused)]
        pub struct $name(PyAny);
        impl pyo3::ToPyErr for $name {}

        pyobject_native_type_core!(
            $name,
            $name::type_object_raw,
            #module=Some("pyreqwest.exceptions")
        );

        impl $name {
            fn type_object_raw(py: Python<'_>) -> *mut pyo3::ffi::PyTypeObject {
                static TYPE_OBJECT: GILOnceCell<Py<PyType>> = GILOnceCell::new();
                TYPE_OBJECT
                    .import(py, "pyreqwest.exceptions", stringify!($name))
                    .unwrap_or_else(|e| panic!("failed to import exception {}: {}", stringify!($name), e))
                    .as_type_ptr()
            }

            #[allow(unused)]
            pub fn new_err(message: &str, details: Option<serde_json::Value>) -> PyErr {
                PyErr::new::<Self, _>((message.to_string(), JsonValue(details.unwrap_or_else(|| json!({})))))
            }

            #[allow(unused)]
            pub fn from_err<E: Error>(message: &str, err: &E) -> PyErr {
                let details = json!({"causes": causes(err)});
                Self::new_err(message, Some(details))
            }
        }
    }
}

define_exception!(HTTPError);

define_exception!(RequestError);
define_exception!(TransportError);
define_exception!(DecodeError);
define_exception!(RedirectError);
define_exception!(StatusError);

define_exception!(RequestTimeoutError);
define_exception!(NetworkError);

define_exception!(ConnectTimeoutError);
define_exception!(ReadTimeoutError);
define_exception!(WriteTimeoutError);
define_exception!(PoolTimeoutError);

define_exception!(ConnectError);
define_exception!(ReadError);
define_exception!(WriteError);
define_exception!(CloseError);

define_exception!(BuilderError);

fn causes<E: Error>(err: E) -> Vec<String> {
    let mut causes: Vec<String> = Vec::new();
    while let Some(source) = err.source() {
        causes.push(source.to_string());
    }
    causes
}
