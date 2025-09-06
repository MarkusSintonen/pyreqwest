import gc
import weakref
from collections.abc import AsyncGenerator
from contextvars import ContextVar

import pytest
from pyreqwest.client import Client, ClientBuilder
from pyreqwest.http import RequestBody
from pyreqwest.middleware import Next
from pyreqwest.middleware.types import Middleware
from pyreqwest.request import Request
from pyreqwest.response import Response, ResponseBuilder

from tests.servers.echo_server import EchoServer
from tests.servers.server import Server


def build_client(middleware: Middleware) -> Client:
    return ClientBuilder().with_middleware(middleware).error_for_status(True).build()


async def test_single(echo_server: Server) -> None:
    async def middleware(request: Request, next_handler: Next) -> Response:
        assert request.method == "GET"
        assert request.url == echo_server.url
        assert request.headers["x-test"] == "Val1"
        assert request.extensions["Ext"] == "ExtVal1"

        request.headers["x-test"] = "Val2"
        request.headers["X-Middleware1"] = "Val3"
        request.extensions["Ext"] = "ExtVal2"

        res = await next_handler.run(request)

        assert res.extensions["Ext"] == "ExtVal2"
        assert res.status == 200
        assert res.version == "HTTP/1.1"
        res.headers["X-Middleware2"] = "Val4"
        res.extensions["Ext"] = "ExtVal3"

        return res

    resp = await (
        build_client(middleware)
        .get(echo_server.url)
        .header("X-Test", "Val1")
        .extensions({"Ext": "ExtVal1"})
        .build_consumed()
        .send()
    )

    assert ["x-test", "Val2"] in (await resp.json())["headers"]
    assert ["x-middleware1", "Val3"] in (await resp.json())["headers"]
    assert resp.headers["X-Middleware2"] == "Val4"
    assert resp.extensions["Ext"] == "ExtVal3"


@pytest.mark.parametrize("reverse", [False, True])
async def test_multiple(echo_server: Server, reverse: bool) -> None:
    async def middleware1(request: Request, next_handler: Next) -> Response:
        request.headers["X-Middleware1"] = "Applied1"
        return await next_handler.run(request)

    async def middleware2(request: Request, next_handler: Next) -> Response:
        request.headers["X-Middleware2"] = "Applied2"
        return await next_handler.run(request)

    middlewares = [middleware1, middleware2]
    if reverse:
        middlewares.reverse()

    builder = ClientBuilder().error_for_status(True)
    for middleware in middlewares:
        builder = builder.with_middleware(middleware)
    client = builder.build()

    resp = await client.get(echo_server.url).build_consumed().send()

    headers = [h for h in (await resp.json())["headers"] if h[0].startswith("x-")]
    if reverse:
        assert headers == [["x-middleware2", "Applied2"], ["x-middleware1", "Applied1"]]
    else:
        assert headers == [["x-middleware1", "Applied1"], ["x-middleware2", "Applied2"]]


async def test_context_vars(echo_server: Server) -> None:
    ctx_var = ContextVar("test_var", default="default_value")

    async def middleware(request: Request, next_handler: Next) -> Response:
        assert ctx_var.get() == "val1"
        res = await next_handler.run(request)
        ctx_var.set("val2")
        return res

    client = build_client(middleware)

    ctx_var.set("val1")

    await client.get(echo_server.url).build_consumed().send()

    assert ctx_var.get() == "val1"


@pytest.mark.parametrize("before", [False, True])
async def test_raise_error(echo_server: Server, before: bool) -> None:
    async def middleware(request: Request, next_handler: Next) -> Response:
        if before:
            raise ValueError("Test error")
        res = await next_handler.run(request)
        if not before:
            raise ValueError("Test error")
        return res

    req = build_client(middleware).get(echo_server.url).build_consumed()

    with pytest.raises(ValueError, match="Test error"):
        await req.send()


async def test_multi_run_error(echo_server: Server) -> None:
    calls = []

    async def middleware(request: Request, next_handler: Next) -> Response:
        await next_handler.run(request)
        calls.append("run1")
        res = await next_handler.run(request)
        calls.append("run2")
        return res

    req = build_client(middleware).get(echo_server.url).build_consumed()
    with pytest.raises(RuntimeError, match="Request was already sent"):
        await req.send()
    assert calls == ["run1"]


async def test_bad_middleware(echo_server: Server) -> None:
    async def wrong_args(_request: Request) -> Response:
        pytest.fail("Should not be called")

    req = build_client(wrong_args).get("http://foo.invalid").build_consumed()  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="takes 1 positional argument but 2 were given"):
        await req.send()

    dummy_resp = await ClientBuilder().error_for_status(True).build().get(echo_server.url).build_consumed().send()

    def not_async(_request: Request, _next_handler: Next) -> Response:
        return dummy_resp

    req = build_client(not_async).get("http://foo.invalid").build_consumed()  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="a coroutine was expected"):
        await req.send()

    async def none_return(request: Request, next_handler: Next) -> Response:  # type: ignore[return]
        await next_handler.run(request)

    req = build_client(none_return).get(echo_server.url).build_consumed()
    with pytest.raises(TypeError, match="'NoneType' object cannot be converted to 'Response'"):
        await req.send()


async def test_retry_middleware(echo_server: EchoServer) -> None:
    echo_server.calls = 0

    async def retry_middleware(request: Request, next_handler: Next) -> Response:
        request2 = request.copy()
        await next_handler.run(request)
        return await next_handler.run(request2)

    await build_client(retry_middleware).get(echo_server.url).build_consumed().send()
    assert echo_server.calls == 2


async def test_modify_status(echo_server: EchoServer) -> None:
    async def modify_response(request: Request, next_handler: Next) -> Response:
        resp = await next_handler.run(request)
        assert resp.status == 200
        resp.status = 201
        return resp

    resp = await build_client(modify_response).post(echo_server.url).body_bytes(b"test").build_consumed().send()
    assert resp.status == 201


async def test_modify_body(echo_server: EchoServer) -> None:
    async def modify_body(request: Request, next_handler: Next) -> Response:
        assert request.body is not None
        bytes_ = request.body.copy_bytes()
        assert bytes_ is not None and bytes_.to_bytes() == b"test"
        request.body = RequestBody.from_bytes(bytes_.to_bytes() + b" modified")
        return await next_handler.run(request)

    resp = await build_client(modify_body).post(echo_server.url).body_bytes(b"test").build_consumed().send()
    assert (await resp.json())["body_parts"] == ["test modified"]


async def test_stream_to_body_bytes(echo_server: EchoServer) -> None:
    async def stream_to_body(request: Request, next_handler: Next) -> Response:
        assert request.body is not None
        stream = request.body.get_stream()
        assert stream is not None

        body_parts = [bytes(part).decode() async for part in stream]

        request.body = RequestBody.from_bytes("---".join(body_parts).encode())
        return await next_handler.run(request)

    async def stream_gen() -> AsyncGenerator[bytes]:
        yield b"test1"
        yield b"test2"

    resp = await build_client(stream_to_body).post(echo_server.url).body_stream(stream_gen()).build_consumed().send()
    assert (await resp.json())["body_parts"] == ["test1---test2"]


async def test_stream_modify_body(echo_server: EchoServer) -> None:
    async def modify_stream(request: Request, next_handler: Next) -> Response:
        assert request.body is not None
        stream = request.body.get_stream()
        assert stream is not None

        async def stream_gen2() -> AsyncGenerator[bytes]:
            async for part in stream:
                yield (bytes(part).decode() + " modified").encode()

        request.body = RequestBody.from_stream(stream_gen2())
        return await next_handler.run(request)

    async def stream_gen() -> AsyncGenerator[bytes]:
        yield b"test1"
        yield b"test2"

    resp = await build_client(modify_stream).post(echo_server.url).body_stream(stream_gen()).build_consumed().send()
    assert (await resp.json())["body_parts"] == ["test1 modified", "test2 modified"]


async def test_stream_context_var(echo_server: EchoServer) -> None:
    ctx_var = ContextVar("test_var", default="default_value")

    async def modify_stream(request: Request, next_handler: Next) -> Response:
        assert request.body is not None
        stream = request.body.get_stream()
        assert stream is not None

        async def stream_gen2() -> AsyncGenerator[bytes]:
            assert ctx_var.get() == "val1"
            async for part in stream:
                yield (bytes(part).decode() + " modified").encode()

        request.body = RequestBody.from_stream(stream_gen2())
        return await next_handler.run(request)

    async def stream_gen() -> AsyncGenerator[bytes]:
        yield b"test1"
        yield b"test2"

    ctx_var.set("val1")

    resp = await build_client(modify_stream).post(echo_server.url).body_stream(stream_gen()).build_consumed().send()
    assert (await resp.json())["body_parts"] == ["test1 modified", "test2 modified"]


@pytest.mark.parametrize("body_stream", [False, True])
async def test_override_with_response_builder(body_stream: bool) -> None:
    async def override_response(_request: Request, _next_handler: Next) -> Response:
        builder = ResponseBuilder().status(201)

        if body_stream:

            async def stream_gen() -> AsyncGenerator[bytes]:
                yield b"test "
                yield b"override"

            builder.body_stream(stream_gen())
        else:
            builder.body_text("test override")

        return await builder.build()

    resp = await build_client(override_response).get("http://foo.invalid").build_consumed().send()
    assert resp.status == 201
    assert (await resp.text()) == "test override"


async def test_response_builder_stream_context_var() -> None:
    context_var = ContextVar("test_var", default="default_value")

    async def override_response(_request: Request, _next_handler: Next) -> Response:
        async def stream_gen() -> AsyncGenerator[bytes]:
            assert context_var.get() == "val1"
            yield b"test "
            yield b"override"

        return await ResponseBuilder().status(201).body_stream(stream_gen()).build()

    context_var.set("val1")

    resp = await build_client(override_response).get("http://foo.invalid").build_consumed().send()
    assert resp.status == 201
    assert (await resp.text()) == "test override"


async def test_proxy_nested_request(echo_server: EchoServer) -> None:
    echo_server.calls = 0

    class MiddlewareProxy:
        def __init__(self) -> None:
            self.client: Client | None = None

        async def __call__(self, request: Request, next_handler: Next) -> Response:
            if request.extensions.get("skip_proxy"):
                return await next_handler.run(request)
            assert request.url == "http://foo.invalid"
            ext = {"skip_proxy": True}
            assert self.client
            return await self.client.request(request.method, echo_server.url).extensions(ext).build_consumed().send()

    middleware = MiddlewareProxy()
    client = build_client(middleware)
    middleware.client = client

    resp = await client.get("http://foo.invalid").build_consumed().send()
    assert (await resp.json())["method"] == "GET"
    assert echo_server.calls == 1


async def test_nested_request_context_var(echo_server: EchoServer) -> None:
    ctx_var = ContextVar("test_var", default="default_value")

    class MiddlewareProxyCtxVar:
        def __init__(self) -> None:
            self.client: Client | None = None

        async def __call__(self, request: Request, next_handler: Next) -> Response:
            if ctx_var.get() == "val1":
                ctx_var.set("val2")
                assert self.client
                return await self.client.request(request.method, echo_server.url).build_consumed().send()
            assert ctx_var.get() == "val2"
            return await next_handler.run(request)

    middleware = MiddlewareProxyCtxVar()

    ctx_var.set("val1")

    client = build_client(middleware)
    middleware.client = client

    resp = await client.get("http://foo.invalid").build_consumed().send()
    assert (await resp.json())["method"] == "GET"


async def test_proxy_modify_request(echo_server: EchoServer) -> None:
    echo_server.calls = 0

    class MiddlewareProxy:
        def __init__(self) -> None:
            self.client: Client | None = None

        async def __call__(self, request: Request, next_handler: Next) -> Response:
            if request.url == "http://foo.invalid":
                request.url = echo_server.url
            return await next_handler.run(request)

    middleware = MiddlewareProxy()
    client = build_client(middleware)
    middleware.client = client

    await client.get("http://foo.invalid").build_consumed().send()
    assert echo_server.calls == 1


async def test_mocking_via_middleware(monkeypatch: pytest.MonkeyPatch) -> None:
    mocked_ids: set[int] = set()
    orig_build = ClientBuilder.build

    def build_patch(self: ClientBuilder) -> Client:
        if id(self) in mocked_ids:  # Break recursion
            mocked_ids.remove(id(self))
            return orig_build(self)

        async def mock_request(request: Request, _next_handler: Next) -> Response:
            assert request.url == "http://foo.invalid" and request.method == "GET"
            return await ResponseBuilder().status(202).body_text("Mocked").build()

        mocked_ids.add(id(self))
        return self.with_middleware(mock_request).build()

    monkeypatch.setattr(ClientBuilder, "build", build_patch)

    client = ClientBuilder().error_for_status(True).build()
    resp = await client.get("http://foo.invalid").build_consumed().send()
    assert resp.status == 202 and (await resp.text()) == "Mocked"


async def test_circular_reference_collected(echo_server: EchoServer) -> None:
    # Check that client has GC support via __traverse__ and __clear__
    ref: weakref.ReferenceType[Middleware] | None = None

    async def check() -> None:
        nonlocal ref

        class MiddlewareWithClient:
            def __init__(self) -> None:
                self.client: Client | None = None

            async def __call__(self, request: Request, next_handler: Next) -> Response:
                return await next_handler.run(request)

        middleware = MiddlewareWithClient()
        client = build_client(middleware)
        middleware.client = client

        ref = weakref.ref(middleware)

        await client.get(echo_server.url).build_consumed().send()

    await check()
    gc.collect()
    assert ref is not None and ref() is None
