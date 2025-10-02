use crate::exceptions::utils::error_causes_iter;
use crate::internal::types::JsonValue;
use pyo3::prelude::*;
use pyo3::pyobject_native_type_core;
use pyo3::sync::PyOnceLock;
use pyo3::types::PyType;
use serde_json::json;
use std::any::Any;
use std::error::Error;

macro_rules! define_exception {
    ($name:ident) => {
        #[allow(unused)]
        pub struct $name(PyAny);
        impl pyo3::ToPyErr for $name {}

        pyobject_native_type_core!($name, $name::type_object_raw, #module=Some("pyreqwest.exceptions"));

        impl $name {
            fn type_object_raw(py: Python<'_>) -> *mut pyo3::ffi::PyTypeObject {
                static TYPE_OBJECT: PyOnceLock<Py<PyType>> = PyOnceLock::new();
                TYPE_OBJECT
                    .import(py, "pyreqwest.exceptions", stringify!($name))
                    .unwrap_or_else(|e| panic!("failed to import exception {}: {}", stringify!($name), e))
                    .as_type_ptr()
            }

            #[allow(unused)]
            pub fn from_causes(message: &str, err_causes: Vec<&'_ (dyn Error + 'static)>) -> PyErr {
                PyErr::new::<Self, _>((message.to_string(), details_from_causes(err_causes.into_iter())))
            }

            #[allow(unused)]
            pub fn from_err<E: Error + 'static>(message: &str, err: &E) -> PyErr {
                PyErr::new::<Self, _>((message.to_string(), details_from_err(err)))
            }

            #[allow(unused)]
            pub fn from_custom(message: &str, details: serde_json::Value) -> PyErr {
                PyErr::new::<Self, _>((message.to_string(), JsonValue(details)))
            }

            #[allow(unused)]
            pub fn from_panic_payload(message: &str, payload: Box<dyn Any>) -> PyErr {
                PyErr::new::<Self, _>((message.to_string(), details_from_panic(payload)))
            }
        }
    }
}

define_exception!(RequestError);

define_exception!(StatusError);
define_exception!(RedirectError);

define_exception!(BodyDecodeError);
define_exception!(DecodeError);
define_exception!(JSONDecodeError);

define_exception!(TransportError);
define_exception!(RequestTimeoutError);
define_exception!(NetworkError);

define_exception!(ConnectTimeoutError);
define_exception!(ReadTimeoutError);
define_exception!(WriteTimeoutError);
define_exception!(PoolTimeoutError);

define_exception!(ConnectError);
define_exception!(ReadError);
define_exception!(WriteError);

define_exception!(ClientClosedError);
define_exception!(BuilderError);
define_exception!(RequestPanicError);

fn details_from_err(err: &(dyn Error + 'static)) -> JsonValue {
    details_from_causes(error_causes_iter(err))
}

fn details_from_panic(payload: Box<dyn Any>) -> JsonValue {
    if let Some(s) = payload.downcast_ref::<String>() {
        return JsonValue(json!({"causes": [s]}));
    }
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        return JsonValue(json!({"causes": [s]}));
    }
    if let Some(e) = payload.downcast_ref::<PyErr>() {
        return details_from_err(e);
    }
    JsonValue(json!({"causes": serde_json::Value::Null}))
}

fn details_from_causes<'a>(err_causes: impl Iterator<Item = &'a (dyn Error + 'static)>) -> JsonValue {
    let causes: Vec<serde_json::value::Value> = err_causes.map(|e| json!({"message": e.to_string()})).collect();
    let causes = if causes.is_empty() { None } else { Some(causes) };
    JsonValue(json!({"causes": causes}))
}
