"""Middleware usage.

Run directly:
    uv run python -m examples.middleware
"""

import asyncio
import sys
from collections.abc import AsyncIterator
from datetime import timedelta
from typing import Self

from pyreqwest.client import ClientBuilder
from pyreqwest.exceptions import ConnectTimeoutError
from pyreqwest.middleware import Next
from pyreqwest.request import Request, RequestBody
from pyreqwest.response import Response, ResponseBuilder

from ._utils import httpbin_url, parse_data_uri, run_examples


async def example_simple() -> None:
    """Single middleware modifying headers"""

    async def mw(request: Request, next_handler: Next) -> Response:
        request.headers["X-Mw"] = "val1"
        resp = await next_handler.run(request)
        resp.headers["X-Mw-Resp"] = "val2"
        return resp

    async with ClientBuilder().with_middleware(mw).error_for_status(True).build() as client:
        resp = await client.get(httpbin_url() / "get").build().send()
        data = await resp.json()
        assert data["headers"]["X-Mw"] == ["val1"] and resp.headers["X-Mw-Resp"] == "val2"
        print({"request_header": data["headers"]["X-Mw"], "response_header": resp.headers["X-Mw-Resp"]})


async def example_multiple() -> None:
    """Multiple middleware order"""

    async def mw1(request: Request, next_handler: Next) -> Response:
        request.headers["X-Mw1"] = "1"
        return await next_handler.run(request)

    async def mw2(request: Request, next_handler: Next) -> Response:
        request.headers["X-Mw2"] = "2"
        return await next_handler.run(request)

    async with ClientBuilder().with_middleware(mw1).with_middleware(mw2).error_for_status(True).build() as client:
        resp = await client.get(httpbin_url() / "get").build().send()
        headers = (await resp.json())["headers"]
        print({"order": [headers["X-Mw1"], headers["X-Mw2"]]})


async def example_extensions() -> None:
    """Passing of (protocol) extensions across middlewares. To allow changing per request middleware behavior."""
    req_ext = {"demo_ext": "init"}

    async def mw1(request: Request, next_handler: Next) -> Response:
        request.headers["X-Mw1"] = request.extensions["demo_ext"]
        request.extensions["demo_ext"] += "_mw1"
        return await next_handler.run(request)

    async def mw2(request: Request, next_handler: Next) -> Response:
        request.headers["X-Mw2"] = request.extensions["demo_ext"]
        request.extensions["demo_ext"] += "_mw2"
        return await next_handler.run(request)

    async with ClientBuilder().with_middleware(mw1).with_middleware(mw2).error_for_status(True).build() as client:
        resp = await client.get(httpbin_url() / "get").extensions(req_ext).build().send()
        assert resp.extensions["demo_ext"] == "init_mw1_mw2"
        headers = (await resp.json())["headers"]
        assert headers["X-Mw1"] == ["init"] and headers["X-Mw2"] == ["init_mw1"]
        print({"mws": [headers["X-Mw1"], headers["X-Mw2"]], "extensions": resp.extensions})


async def retry_middleware(request: Request, next_handler: Next) -> Response:
    """A simple retry middleware that retries once on ConnectTimeoutError"""
    request2 = request.copy()  # Copy before first send

    try:
        await next_handler.run(request)
        raise RuntimeError("should have raised")
    except ConnectTimeoutError:
        pass

    request2.url = request2.url.with_path("delay/0")  # Succeeds
    response2 = await next_handler.run(request2)
    response2.extensions["retried"] = True  # Just to show it was retried
    return response2


async def example_retry_middleware() -> None:
    """Retry in a middleware"""
    async with (
        ClientBuilder()
        .with_middleware(retry_middleware)
        .timeout(timedelta(seconds=1))
        .error_for_status(True)
        .build() as client
    ):
        resp = await client.get(httpbin_url() / "delay/2").build().send()  # First timeouts
        assert resp.extensions["retried"] is True
        data = await resp.json()
        print({"url": data["url"], "status": resp.status, "retried": resp.extensions["retried"]})


async def example_retry_middleware_with_stream() -> None:
    """Retry in a middleware with streamed body"""

    # Custom stream must support __copy__ to be retryable
    class MyStream:
        def __init__(self, copied: bool = False) -> None:
            self.copied = copied

        def __aiter__(self) -> AsyncIterator[bytes]:
            if self.copied:

                async def stream() -> AsyncIterator[bytes]:
                    yield b"hello"
                    yield b"_world"
            else:

                async def stream() -> AsyncIterator[bytes]:
                    yield b"not_used"

            return stream()

        def __copy__(self) -> Self:
            return self.__class__(copied=True)

    async with (
        ClientBuilder()
        .with_middleware(retry_middleware)
        .timeout(timedelta(seconds=1))
        .error_for_status(True)
        .build() as client
    ):
        resp = await client.put(httpbin_url() / "delay/2").body_stream(MyStream()).build().send()  # First timeouts
        body = parse_data_uri((await resp.json())["data"])
        assert body == "hello_world"
        print({"data": body, "status": resp.status})


async def example_middleware_modify_request_body() -> None:
    """Modify body bytes before send"""

    async def mw(request: Request, next_handler: Next) -> Response:
        if request.body is not None and (b := request.body.copy_bytes()):
            request.body = RequestBody.from_bytes(b.to_bytes() + b"_world")
        return await next_handler.run(request)

    async with ClientBuilder().with_middleware(mw).error_for_status(True).build() as client:
        resp = await client.post(httpbin_url() / "post").body_bytes(b"hello").build().send()
        data = await resp.json()
        assert parse_data_uri(data["data"]) == "hello_world"
        print({"data": data["data"]})


async def example_middleware_modify_request_body_streamed() -> None:
    """Modify streamed request body"""

    async def mw(request: Request, next_handler: Next) -> Response:
        if request.body is not None and (stream := request.body.get_stream()):
            assert isinstance(stream, AsyncIterator)

            async def gen() -> AsyncIterator[bytes]:
                async for part in stream:
                    yield part
                yield b"_middleware"

            request.body = RequestBody.from_stream(gen())
        return await next_handler.run(request)

    async def src() -> AsyncIterator[bytes]:
        yield b"hello"
        yield b"_world"

    async with ClientBuilder().with_middleware(mw).error_for_status(True).build() as client:
        resp = await client.post(httpbin_url() / "post").body_stream(src()).build().send()
        data = await resp.json()
        assert parse_data_uri(data["data"]) == "hello_world_middleware"
        print({"data": data["data"]})


async def example_middleware_override_response_builder() -> None:
    """Short circuit request and return custom response"""

    async def mw(_request: Request, _next: Next) -> Response:
        return await ResponseBuilder().status(202).body_text("overridden").build()

    async with ClientBuilder().with_middleware(mw).error_for_status(True).build() as client:
        resp = await client.get(httpbin_url() / "get").build().send()
        assert (await resp.text()) == "overridden" and resp.status == 202
        print({"status": resp.status, "body": await resp.text()})


async def example_middleware_request_specific() -> None:
    """Request specific middleware"""

    async def mw1(request: Request, next_handler: Next) -> Response:
        request.url = request.url.extend_query({"mw1": "val1"})
        return await next_handler.run(request)

    async def mw2(request: Request, next_handler: Next) -> Response:
        request.url = request.url.extend_query({"mw2": "val2"})
        return await next_handler.run(request)

    async with ClientBuilder().with_middleware(mw1).error_for_status(True).build() as client:
        req1 = client.get(httpbin_url() / "get").with_middleware(mw2).build()  # Uses both middlewares
        req2 = client.get(httpbin_url() / "get").build()
        resp1, resp2 = await req1.send(), await req2.send()
        assert (await resp1.json())["args"] == {"mw1": ["val1"], "mw2": ["val2"]}
        assert (await resp2.json())["args"] == {"mw1": ["val1"]}
        print({"req1_args": (await resp1.json())["args"], "req2_args": (await resp2.json())["args"]})


if __name__ == "__main__":
    asyncio.run(run_examples(sys.modules[__name__]))
