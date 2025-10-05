"""Exception classes."""

from json import JSONDecodeError as JSONDecodeError_
from typing import Any, Generic, TypedDict, TypeVar


class Cause(TypedDict):
    """A cause of an error."""

    message: str


class CauseErrorDetails(TypedDict):
    """Details for errors that may have causes."""

    causes: list[Cause] | None


class StatusErrorDetails(TypedDict):
    """Details for errors that have an associated HTTP status code."""

    status: int


T = TypeVar("T", bound=CauseErrorDetails | StatusErrorDetails)


class PyreqwestError(Exception):
    """Base class for all pyreqwest errors."""

    def __init__(self, message: str, *args: Any) -> None:
        """Internally initialized."""
        assert isinstance(message, str)
        Exception.__init__(self, message, *args)
        self.message = message


class DetailedPyreqwestError(PyreqwestError, Generic[T]):
    """Base class for all pyreqwest errors with details.

    Details may be available in `details`.
    """

    def __init__(self, message: str, details: T) -> None:
        """Internally initialized."""
        assert isinstance(details, dict)
        PyreqwestError.__init__(self, message, details)
        self.details = details


class RequestError(DetailedPyreqwestError[T], Generic[T]):
    """Error while processing a request.

    Details may be available in `details`.
    """


class StatusError(RequestError[StatusErrorDetails]):
    """Error due to HTTP 4xx or 5xx status code. Raised when `error_for_status` is enabled.

    The status code is available in `details["status"]`.
    """


class RedirectError(RequestError[CauseErrorDetails]):
    """Error due to too many redirects. Raised when `max_redirects` is exceeded.

    Cause details may be available in `details["causes"]`.
    """


class DecodeError(RequestError[CauseErrorDetails]):
    """Error while decoding the response.

    Cause details may be available in `details["causes"]`.
    """


class BodyDecodeError(DecodeError):
    """Error while decoding the request or response body.

    Cause details may be available in `details["causes"]`.
    """


class JSONDecodeError(BodyDecodeError, JSONDecodeError_):
    """Error while decoding the response body as JSON.

    This corresponds to Python's built-in `json.JSONDecodeError`. With the difference that `pos` and `colno` are byte
    offsets instead of UTF8 char offsets. This difference is for efficient error handling (avoiding UTF8 conversions).
    """

    def __init__(self, message: str, details: dict[str, Any]) -> None:
        """Internally initialized."""
        assert isinstance(details, dict)
        assert isinstance(details["doc"], str) and isinstance(details["pos"], int)
        JSONDecodeError_.__init__(self, message, details["doc"], details["pos"])
        BodyDecodeError.__init__(self, message, {"causes": details["causes"]})


class TransportError(RequestError[CauseErrorDetails]):
    """Error while processing the transport layer.

    Cause details may be available in `details["causes"]`.
    """


class RequestTimeoutError(TransportError, TimeoutError):
    """Error due to a timeout.

    This indicates that the timeout configured for the request was reached.
    Cause details may be available in `details["causes"]`.
    """


class NetworkError(TransportError):
    """Error due to a network failure.

    This indicates that the request could not be completed due to a network failure.
    Cause details may be available in `details["causes"]`.
    """


class ConnectTimeoutError(RequestTimeoutError):
    """Timeout while connecting.

    Cause details may be available in `details["causes"]`.
    """


class ReadTimeoutError(RequestTimeoutError):
    """Timeout while reading body.

    Cause details may be available in `details["causes"]`.
    """


class WriteTimeoutError(RequestTimeoutError):
    """Timeout while sending body.

    Cause details may be available in `details["causes"]`.
    """


class PoolTimeoutError(RequestTimeoutError):
    """Timeout while acquiring a connection from the pool.

    Cause details may be available in `details["causes"]`.
    """


class ConnectError(NetworkError):
    """Network error while connecting.

    Cause details may be available in `details["causes"]`.
    """


class ReadError(NetworkError):
    """Network error while reading body.

    Cause details may be available in `details["causes"]`.
    """


class WriteError(NetworkError):
    """Network error while sending body.

    Cause details may be available in `details["causes"]`.
    """


class ClientClosedError(RequestError[CauseErrorDetails]):
    """Error due to user closing the client while request was being processed.

    Cause details may be available in `details["causes"]`.
    """


class BuilderError(DetailedPyreqwestError[CauseErrorDetails], ValueError):
    """Error while building a request.

    Cause details may be available in `details["causes"]`.
    """


class RequestPanicError(RequestError[CauseErrorDetails]):
    """Error due to a panic in the request processing.

    This indicates a bug in pyreqwest or one of its dependencies.
    Also, might be raised due to incorrect ProxyBuilder.custom implementation (limitation in reqwest error handling).
    Cause details may be available in `details["causes"]`.
    """
