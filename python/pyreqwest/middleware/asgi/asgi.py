"""ASGI test client for pyreqwest."""
import asyncio
from typing import Any, Callable
from urllib.parse import unquote

from pyreqwest.client import Client
from pyreqwest.middleware import Next
from pyreqwest.request import Request
from pyreqwest.response import Response, ResponseBuilder


class ASGITestMiddleware:
    """Test client that routes requests through an ASGI application."""

    def __init__(self, app: Callable):
        """Initialize the ASGI test client.

        Args:
            app: ASGI application callable
        """
        self.app = app

    async def __call__(self, client: Client, request: Request, next_handler: Next) -> Response:
        scope = await self._request_to_asgi_scope(request)

        receive_queue: asyncio.Queue = asyncio.Queue()
        send_queue: asyncio.Queue = asyncio.Queue()

        if request.body is not None:
            body_bytes = await self._get_request_body_bytes(request)
            await receive_queue.put({
                "type": "http.request",
                "body": body_bytes,
                "more_body": False
            })
        else:
            await receive_queue.put({
                "type": "http.request",
                "body": b"",
                "more_body": False
            })

        async def receive():
            return await receive_queue.get()

        async def send(message):
            await send_queue.put(message)

        await self.app(scope, receive, send)

        return await self._asgi_response_to_response(send_queue)

    async def _request_to_asgi_scope(self, request: Request) -> dict[str, Any]:
        url = request.url
        return {
            "type": "http",
            "asgi": {"version": "3.0"},
            "http_version": "1.1",
            "method": request.method.upper(),
            "scheme": url.scheme,
            "path": unquote(url.path),
            "query_string": (url.query_string or "").encode(),
            "headers": [[name.lower().encode(), value.encode()] for name, value in request.headers.items()],
            "server": ("testserver", 80 if url.scheme == "http" else 443),
            "client": ("testclient", 12345),
        }

    async def _get_request_body_bytes(self, request: Request) -> bytes:
        if request.body is None:
            return b""

        if (stream := request.body.get_stream()) is not None:
            body_parts = []
            async for chunk in stream:
                body_parts.append(bytes(chunk))
            return b"".join(body_parts)

        body_buf = request.body.copy_bytes()
        assert body_buf is not None, "Unknown body type"
        return body_buf.to_bytes()

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

        # Set the complete body
        if body_parts:
            complete_body = b"".join(body_parts)
            response_builder.body_bytes(complete_body)

        return await response_builder.build()
