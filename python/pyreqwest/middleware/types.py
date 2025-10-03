"""Middleware types and interfaces."""

from collections.abc import Awaitable, Callable

from pyreqwest.middleware import Next, SyncNext
from pyreqwest.request import Request
from pyreqwest.response import Response, SyncResponse

Middleware = Callable[[Request, Next], Awaitable[Response]]
"""Middleware handler which is called with a request before sending it.

Call `await Next.run(Request)` to continue processing the request.
Alternatively, you can return a custom response via `ResponseBuilder`.
If you need to forward data down the middleware stack, you can use Request.extensions.
If you are retrying requests, make sure to clone the request via `Request.copy()` before sending.

Args:
    Request: HTTP request to process
    Next: Next middleware in the chain to call

Returns:
    HTTP response from the next middleware or a custom response.
"""

SyncMiddleware = Callable[[Request, SyncNext], SyncResponse]
"""Sync middleware handler which is used in blocking context. See Middleware for details."""
