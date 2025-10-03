"""ASGI middleware."""

import asyncio
from collections.abc import AsyncIterable, AsyncIterator, Awaitable, Callable, MutableMapping
from datetime import timedelta
from typing import Any, Self
from urllib.parse import unquote

from pyreqwest.middleware import Next
from pyreqwest.request import Request
from pyreqwest.response import Response, ResponseBuilder

Scope = MutableMapping[str, Any]
Message = MutableMapping[str, Any]
Receive = Callable[[], Awaitable[Message]]
Send = Callable[[Message], Awaitable[None]]
ASGIApp = Callable[[Scope, Receive, Send], Awaitable[None]]


class ASGITestMiddleware:
    """Test client that routes requests into an ASGI application."""

    def __init__(
        self,
        app: ASGIApp,
        *,
        timeout: timedelta | None = None,
        scope_update: Callable[[dict[str, Any], Request], Awaitable[None]] | None = None,
    ) -> None:
        """Initialize the ASGI test client.

        Args:
            app: ASGI application callable
            timeout: Timeout for ASGI operations (default: 5 seconds)
            scope_update: Optional coroutine to modify the ASGI scope per request
        """
        self._app = app
        self._scope_update = scope_update
        self._timeout = timeout or timedelta(seconds=5)
        self._lifespan_input_queue: asyncio.Queue[MutableMapping[str, Any]] = asyncio.Queue()
        self._lifespan_output_queue: asyncio.Queue[MutableMapping[str, Any]] = asyncio.Queue()
        self._lifespan_task: asyncio.Task[None] | None = None
        self._state: dict[str, Any] = {}

    async def __aenter__(self) -> Self:
        """Start up the ASGI application."""

        async def wrapped_lifespan() -> None:
            await self._app(
                {"type": "lifespan", "asgi": {"version": "3.0"}, "state": self._state},
                self._lifespan_input_queue.get,
                self._lifespan_output_queue.put,
            )

        self._lifespan_task = asyncio.create_task(wrapped_lifespan())
        await self._send_lifespan("startup")
        return self

    async def __aexit__(self, *args: object, **kwargs: Any) -> None:
        """Shutdown the ASGI application."""
        await self._send_lifespan("shutdown")
        self._lifespan_task = None

    async def _send_lifespan(self, action: str) -> None:
        assert self._lifespan_task

        await self._lifespan_input_queue.put({"type": f"lifespan.{action}"})
        message = await asyncio.wait_for(self._lifespan_output_queue.get(), timeout=self._timeout.total_seconds())

        if message["type"] == f"lifespan.{action}.failed":
            await asyncio.sleep(0)
            if self._lifespan_task.done() and (exc := self._lifespan_task.exception()) is not None:
                raise exc
            raise LifespanError(message)

    async def __call__(self, request: Request, _next_handler: Next) -> Response:
        """ASGI middleware handler."""
        scope = await self._request_to_asgi_scope(request)
        body_parts = self._asgi_body_parts(request)

        send_queue: asyncio.Queue[MutableMapping[str, Any]] = asyncio.Queue()

        async def receive() -> dict[str, Any]:
            if part := await anext(body_parts, None):
                return part
            return {"type": "http.disconnect"}

        async def send(message: MutableMapping[str, Any]) -> None:
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
            "state": self._state.copy(),
        }
        if self._scope_update is not None:
            await self._scope_update(scope, request)
        return scope

    async def _asgi_body_parts(self, request: Request) -> AsyncIterator[dict[str, Any]]:
        if request.body is None:
            yield {"type": "http.request", "body": b"", "more_body": False}
            return

        if (stream := request.body.get_stream()) is not None:
            assert isinstance(stream, AsyncIterable)
            body_parts = [bytes(chunk) async for chunk in stream]
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

    async def _asgi_response_to_response(self, send_queue: asyncio.Queue[MutableMapping[str, Any]]) -> Response:
        response_builder = ResponseBuilder()
        body_parts = []

        while True:
            message = await asyncio.wait_for(send_queue.get(), timeout=self._timeout.total_seconds())

            if message["type"] == "http.response.start":
                response_builder.status(message["status"])
                response_builder.headers([(k.decode(), v.decode()) for k, v in message.get("headers", [])])

            elif message["type"] == "http.response.body":
                if body := message.get("body"):
                    body_parts.append(body)

                if not message.get("more_body", False):
                    break

        if len(body_parts) > 1:

            async def body_stream() -> AsyncIterator[bytes]:
                for part in body_parts:
                    yield part

            response_builder.body_stream(body_stream())

        elif len(body_parts) == 1:
            response_builder.body_bytes(body_parts[0])

        return await response_builder.build()


class LifespanError(Exception):
    """Error during ASGI application lifespan events."""
