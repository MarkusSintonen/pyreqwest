"""ASGI test client for pyreqwest."""
import asyncio
from typing import Any, Callable, Iterator, AsyncGenerator, AsyncIterator, Coroutine
from urllib.parse import unquote

from pyreqwest.client import Client
from pyreqwest.http import Body
from pyreqwest.middleware import Next
from pyreqwest.request import Request
from pyreqwest.response import Response, ResponseBuilder


class ASGITestMiddleware:
    """Test client that routes requests through an ASGI application."""

    def __init__(
        self,
        app: Callable,
        *,
        scope_update: Callable[[dict[str, Any], Request], Coroutine[Any, Any, None]] | None = None,
    ):
        """Initialize the ASGI test client.

        Args:
            app: ASGI application callable
        """
        self._app = app
        self._scope_update = scope_update

    async def __call__(self, client: Client, request: Request, next_handler: Next) -> Response:
        scope = await self._request_to_asgi_scope(request)
        body_parts = self._asgi_body_parts(request)

        send_queue: asyncio.Queue = asyncio.Queue()

        async def receive():
            if part := await anext(body_parts, None):
                return part
            return {"type": "http.disconnect"}

        async def send(message):
            await send_queue.put(message)

        await self._app(scope, receive, send)

        return await self._asgi_response_to_response(send_queue)

    async def _request_to_asgi_scope(self, request: Request) -> dict[str, Any]:
        url = request.url
        scope = {
            "type": "http",
            "asgi": {"version": "3.0"},
            "http_version": "1.1",
            "method": request.method.upper(),
            "scheme": url.scheme,
            "path": unquote(url.path),
            "raw_path": url.path.encode(),
            "root_path": "",
            "query_string": (url.query_string or "").encode(),
            "headers": [[name.lower().encode(), value.encode()] for name, value in request.headers.items()],
        }
        if self._scope_update is not None:
            await self._scope_update(scope, request)
        return scope

    async def _asgi_body_parts(self, request: Request) -> AsyncIterator[dict[str, Any]]:
        if request.body is None:
            yield {"type": "http.request", "body": b"", "more_body": False}
            return

        if (stream := request.body.get_stream()) is not None:
            body_parts = []
            async for chunk in stream:
                body_parts.append(bytes(chunk))
            if not body_parts:
                yield {"type": "http.request", "body": b"", "more_body": False}
                return
            *parts, last = body_parts
            for part in parts:
                yield {"type": "http.request", "body": part, "more_body": True}
            yield {"type": "http.request", "body": last, "more_body": False}
            return

        body_buf = request.body.copy_bytes()
        assert body_buf is not None, "Unknown body type"
        yield {"type": "http.request", "body": body_buf.to_bytes(), "more_body": False}

    async def _asgi_response_to_response(self, send_queue: asyncio.Queue) -> Response:
        response_builder = ResponseBuilder.create_for_mocking()
        body_parts = []

        while True:
            try:
                message = await asyncio.wait_for(send_queue.get(), timeout=1.0)
            except asyncio.TimeoutError:
                break

            if message["type"] == "http.response.start":
                response_builder.status(message["status"])

                for header_name, header_value in message.get("headers", []):
                    response_builder.header(header_name.decode(), header_value.decode())

            elif message["type"] == "http.response.body":
                if body := message.get("body"):
                    body_parts.append(body)

                if not message.get("more_body", False):
                    break

        if len(body_parts) > 1:
            async def body_stream() -> AsyncIterator[bytes]:
                for part in body_parts:
                    yield part
            response_builder.body(Body.from_stream(body_stream()))

        elif len(body_parts) == 1:
            response_builder.body_bytes(body_parts[0])

        return await response_builder.build()
