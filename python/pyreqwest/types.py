from collections.abc import AsyncIterable, Mapping, Sequence
from typing import Any, Protocol

from pyreqwest.client import Client
from pyreqwest.http import Url
from pyreqwest.middleware import Next
from pyreqwest.request import Request
from pyreqwest.response import Response

UrlType = Url | str
HeadersType = Mapping[str, str] | Sequence[tuple[str, str]]
QueryParams = Mapping[str, Any] | Sequence[tuple[str, Any]]
FormParams = Mapping[str, Any] | Sequence[tuple[str, Any]]
ExtensionsType = Mapping[str, Any] | Sequence[tuple[str, Any]]
Stream = AsyncIterable[bytes] | AsyncIterable[bytearray] | AsyncIterable[memoryview]


class Middleware(Protocol):
    """Middleware interface for processing HTTP requests and responses."""

    async def __call__(self, client: Client, request: Request, next_handler: Next) -> Response:
        """Invoked with a request before sending it.
        Call `await next_handler.run(request)` to continue processing the request.
        Alternatively, you can return a custom response via `next_handler.response_builder` You can also use `client`
        to send additional request(s).
        If you need to forward data down the middleware stack, you can use request.extensions.

        Args:
            client: HTTP client instance
            request: HTTP request to process
            next_handler: Next middleware in the chain to call

        Returns:
            HTTP response from the next middleware or a custom response.
        """
        ...
