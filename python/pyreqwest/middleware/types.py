"""Middleware types and interfaces."""

from collections.abc import Callable, Coroutine
from typing import Any

from pyreqwest.client import Client
from pyreqwest.middleware import Next
from pyreqwest.request import Request
from pyreqwest.response import Response

Middleware = Callable[[Client, Request, Next], Coroutine[Any, Any, Response]]
"""Middleware handler which is called with a request before sending it.

Call `await Next.run(Request)` to continue processing the request.
Alternatively, you can return a custom response via `Next.response_builder` You can also use `Client`
to send additional request(s).
If you need to forward data down the middleware stack, you can use Request.extensions.

Args:
    Client: HTTP client instance
    Request: HTTP request to process
    Next: Next middleware in the chain to call

Returns:
    HTTP response from the next middleware or a custom response.
"""
