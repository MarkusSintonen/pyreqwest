use pyo3::create_exception;
use pyo3::exceptions::PyException;

create_exception!(module, RequestError, PyException);

create_exception!(module, SendError, RequestError);
create_exception!(module, SendConnectionError, SendError);
create_exception!(module, SendBodyError, SendError);
create_exception!(module, SendTimeoutError, SendError);
create_exception!(module, PoolTimeoutError, SendTimeoutError);

create_exception!(module, ReadError, RequestError);
create_exception!(module, ReadBodyError, ReadError);
create_exception!(module, ReadTimeoutError, ReadError);
