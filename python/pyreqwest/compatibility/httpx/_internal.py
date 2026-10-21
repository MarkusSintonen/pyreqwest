from datetime import timedelta
from typing import TypeVar

import httpx

from pyreqwest import exceptions
from pyreqwest.exceptions import PyreqwestError
from pyreqwest.request import BaseRequestBuilder
from pyreqwest.response import BaseResponse

T = TypeVar("T", bound=BaseRequestBuilder)

EXCEPTION_MAPPING: dict[type[PyreqwestError], type[httpx.RequestError]] = {
    exceptions.ConnectTimeoutError: httpx.ConnectTimeout,
    exceptions.ReadTimeoutError: httpx.ReadTimeout,
    exceptions.WriteTimeoutError: httpx.WriteTimeout,
    exceptions.PoolTimeoutError: httpx.PoolTimeout,
    exceptions.RequestTimeoutError: httpx.TimeoutException,
    exceptions.ConnectError: httpx.ConnectError,
    exceptions.ReadError: httpx.ReadError,
    exceptions.WriteError: httpx.WriteError,
    exceptions.NetworkError: httpx.NetworkError,
    exceptions.DecodeError: httpx.DecodingError,
    exceptions.RedirectError: httpx.TooManyRedirects,
}


def build_httpx_response(
    response: BaseResponse, response_stream: httpx.SyncByteStream | httpx.AsyncByteStream | None
) -> httpx.Response:
    return httpx.Response(
        status_code=response.status,
        headers=list(response.headers.items()),
        stream=response_stream,
        extensions={"pyreqwest_response": response},
    )


def _request_timeout(timeout: dict[str, float | None]) -> float:
    return sum(value for value in timeout.values() if value is not None)


def map_extensions(builder: T, request: httpx.Request) -> T:
    if not (ext := request.extensions):
        return builder

    # Handle timeout
    # reqwest differs https://docs.rs/reqwest/latest/reqwest/struct.RequestBuilder.html#method.timeout
    # For more granular control, users should configure the pyreqwest.Client directly.
    if isinstance(timeout := ext.get("timeout"), dict) and (total := _request_timeout(timeout)) > 0:
        builder = builder.timeout(timedelta(seconds=total))

    return builder.extensions(ext)


def map_exception(exc: PyreqwestError, request: httpx.Request) -> httpx.RequestError | None:
    if exact := EXCEPTION_MAPPING.get(type(exc)):
        return exact(exc.message, request=request)

    for pyreqwest_exc, httpx_exc in EXCEPTION_MAPPING.items():
        if isinstance(exc, pyreqwest_exc):
            return httpx_exc(exc.message, request=request)

    return None
