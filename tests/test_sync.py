import copy
import json
import string
from collections.abc import Generator, Iterator
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from contextvars import ContextVar
from datetime import timedelta
from typing import Any, TypeVar

import pytest
from pyreqwest.client import BaseClient, BaseClientBuilder, SyncClient, SyncClientBuilder
from pyreqwest.client.types import SyncJsonLoadsContext
from pyreqwest.exceptions import ClientClosedError, PoolTimeoutError
from pyreqwest.http import HeaderMap
from pyreqwest.middleware import SyncNext
from pyreqwest.middleware.types import SyncMiddleware
from pyreqwest.request import BaseRequestBuilder, Request, SyncConsumedRequest, SyncRequestBuilder, SyncStreamRequest
from pyreqwest.response import BaseResponse, SyncResponse, SyncResponseBodyReader

from tests.servers.server_subprocess import SubprocessServer

T = TypeVar("T")


def client_builder() -> SyncClientBuilder:
    return SyncClientBuilder().error_for_status(True).timeout(timedelta(seconds=5))


@contextmanager
def middleware_client(middleware: SyncMiddleware) -> Generator[SyncClient, None, None]:
    with client_builder().with_middleware(middleware).build() as client:
        yield client


@pytest.fixture
def client() -> Generator[SyncClient, None, None]:
    with client_builder().build() as client:
        yield client


def test_send(client: SyncClient, echo_server: SubprocessServer) -> None:
    assert client.get(echo_server.url).build().send().json()["method"] == "GET"


@pytest.mark.parametrize("str_url", [False, True])
def test_http_methods(echo_server: SubprocessServer, str_url: bool):
    url = str(echo_server.url) if str_url else echo_server.url
    with SyncClientBuilder().error_for_status(True).build() as client:
        with client.get(url).build_streamed() as response:
            assert response.json()["method"] == "GET"
            assert response.json()["scheme"] == "http"
        with client.post(url).build_streamed() as response:
            assert response.json()["method"] == "POST"
        with client.put(url).build_streamed() as response:
            assert response.json()["method"] == "PUT"
        with client.patch(url).build_streamed() as response:
            assert response.json()["method"] == "PATCH"
        with client.delete(url).build_streamed() as response:
            assert response.json()["method"] == "DELETE"
        with client.head(url).build_streamed() as response:
            assert response.headers["content-type"] == "application/json"
        with client.request("QUERY", url).build_streamed() as response:
            assert response.json()["method"] == "QUERY"


def test_read(client: SyncClient, echo_body_parts_server: SubprocessServer) -> None:
    chars = string.ascii_letters + string.digits
    body = b"".join(chars[v % len(chars)].encode() for v in range(131072))

    def stream_gen() -> Iterator[bytes]:
        yield body

    resp = client.post(echo_body_parts_server.url).body_stream(stream_gen()).build().send()
    assert resp.body_reader.read() == body[:65536]
    assert resp.body_reader.read() == body[65536:]
    assert resp.body_reader.read() is None

    resp = client.post(echo_body_parts_server.url).body_stream(stream_gen()).build().send()
    assert resp.body_reader.read(0) == b""
    assert resp.body_reader.read(100) == body[:100]
    assert resp.body_reader.read(100) == body[100:200]
    assert resp.body_reader.read(131072) == body[200:]
    assert resp.body_reader.read(10) is None


def test_middleware(echo_server: SubprocessServer) -> None:
    def middleware(request: Request, next_handler: SyncNext) -> SyncResponse:
        request.headers["x-test1"] = "foo"
        response = next_handler.run(request)
        response.headers["x-test2"] = "bar"
        return response

    with middleware_client(middleware) as client:
        resp = client.get(echo_server.url).build().send()
        assert ["x-test1", "foo"] in resp.json()["headers"]
        assert resp.headers["x-test2"] == "bar"


def test_middleware__bad() -> None:
    async def bad_middleware(_request: Request, _next_handler: SyncNext) -> SyncResponse:
        raise RuntimeError("bad middleware")

    with pytest.raises(ValueError, match="Middleware must be a sync function"):
        SyncClientBuilder().with_middleware(bad_middleware)  # type: ignore[arg-type]


def test_middleware__request_specific(echo_server: SubprocessServer) -> None:
    def middleware1(request: Request, next_handler: SyncNext) -> SyncResponse:
        request.extensions["key1"] = "val1"
        return next_handler.run(request)

    def middleware2(request: Request, next_handler: SyncNext) -> SyncResponse:
        request.extensions["key2"] = "val2"
        return next_handler.run(request)

    with middleware_client(middleware1) as client:
        req1 = client.get(echo_server.url).with_middleware(middleware2).build()
        req2 = client.get(echo_server.url).build()
        req1_copy = req1.copy()
        assert req1.send().extensions == {"key1": "val1", "key2": "val2"}
        assert req2.send().extensions == {"key1": "val1"}
        assert req1_copy.send().extensions == {"key1": "val1", "key2": "val2"}

    with SyncClientBuilder().error_for_status(True).build() as client:
        req3 = client.get(echo_server.url).with_middleware(middleware2).build()
        assert req3.send().extensions == {"key2": "val2"}


def test_context_vars(echo_server: SubprocessServer) -> None:
    ctx_var = ContextVar("test_var", default="default_value")

    def middleware(request: Request, next_handler: SyncNext) -> SyncResponse:
        assert ctx_var.get() == "val1"
        res = next_handler.run(request)
        ctx_var.set("val2")
        res.headers["x-test"] = "foo"
        return res

    with middleware_client(middleware) as client:
        ctx_var.set("val1")
        resp = client.get(echo_server.url).build().send()
        assert resp.headers["x-test"] == "foo"
        assert ctx_var.get() == "val2"


def test_stream(client: SyncClient, echo_body_parts_server: SubprocessServer) -> None:
    def gen() -> Generator[bytes, None, None]:
        for i in range(3):
            yield f"part {i}".encode()

    with client.post(echo_body_parts_server.url).body_stream(gen()).build_streamed() as resp:
        assert resp.body_reader.read_chunk() == b"part 0"
        assert resp.body_reader.read_chunk() == b"part 1"
        assert resp.body_reader.read_chunk() == b"part 2"
        assert resp.body_reader.read_chunk() is None


@pytest.mark.parametrize("call", ["copy", "__copy__"])
@pytest.mark.parametrize("build_streamed", [False, True])
@pytest.mark.parametrize("body_streamed", [False, True])
def test_request_copy(echo_server: SubprocessServer, call: str, build_streamed: bool, body_streamed: bool) -> None:
    def remove_time(ctx: SyncJsonLoadsContext) -> Any:
        d = json.loads(ctx.body_reader.bytes().to_bytes())
        d.pop("time")
        return d

    client = SyncClientBuilder().json_handler(loads=remove_time).error_for_status(True).build()

    builder = client.get(echo_server.url).header("X-Test1", "Val1")
    stream_copied = False

    if body_streamed:

        class StreamGen:
            def __iter__(self) -> Iterator[bytes]:
                def gen() -> Iterator[bytes]:
                    yield b"test1"

                return gen()

            def __copy__(self) -> "StreamGen":
                nonlocal stream_copied
                stream_copied = True
                return StreamGen()

        builder = builder.body_stream(StreamGen())
    else:
        builder = builder.body_text("test1")

    if build_streamed:
        req1: Request = builder.build_streamed()
    else:
        req1 = builder.build()

    if call == "copy":
        req2 = req1.copy()
    else:
        assert call == "__copy__"
        req2 = copy.copy(req1)

    assert req1.method == req2.method == "GET"
    assert req1.url == req2.url
    assert req1.headers["x-test1"] == req2.headers["x-test1"] == "Val1"

    if body_streamed:
        assert stream_copied
        assert req1.body and req2.body and req1.body.get_stream() is not req2.body.get_stream()
    else:
        assert req1.body and req2.body and req1.body.copy_bytes() == req2.body.copy_bytes() == b"test1"
        assert req1.body is not req2.body

    if build_streamed:
        assert isinstance(req1, SyncStreamRequest) and isinstance(req2, SyncStreamRequest)
        with req1 as resp1, req2 as resp2:
            assert resp1.json() == resp2.json()
    else:
        assert isinstance(req1, SyncConsumedRequest) and isinstance(req2, SyncConsumedRequest)
        assert req1.send().json() == req2.send().json()


@pytest.mark.parametrize("concurrency", [1, 2, 10])
@pytest.mark.parametrize("limit", [None, 1, 2, 10])
def test_concurrent_requests(echo_server: SubprocessServer, concurrency: int, limit: int | None) -> None:
    builder = client_builder()
    if limit is not None:
        builder = builder.max_connections(limit)

    with builder.build() as client, ThreadPoolExecutor(max_workers=10) as executor:
        futures = [
            executor.submit(lambda: client.get(echo_server.url).build().send().json()) for _ in range(concurrency)
        ]
        assert all(fut.result()["method"] == "GET" for fut in futures)


@pytest.mark.parametrize("max_conn", [1, 2, None])
def test_max_connections_pool_timeout(echo_server: SubprocessServer, max_conn: int | None):
    url = echo_server.url.with_query({"sleep_start": 0.1})

    builder = client_builder().max_connections(max_conn).pool_timeout(timedelta(seconds=0.05))

    with builder.build() as client, ThreadPoolExecutor(max_workers=10) as executor:
        futures = [executor.submit(lambda: client.get(url).build().send().json()) for _ in range(2)]
        if max_conn == 1:
            with pytest.raises(PoolTimeoutError) as e:
                _ = [fut.result() for fut in futures]
            assert isinstance(e.value, TimeoutError)
        else:
            assert all(fut.result()["method"] == "GET" for fut in futures)


def test_json_loads_callback(echo_server: SubprocessServer):
    called = 0

    def custom_loads(ctx: SyncJsonLoadsContext) -> Any:
        nonlocal called
        called += 1
        assert ctx.headers["Content-Type"] == "application/json"
        assert ctx.extensions == {"my_ext": "foo"}
        content = ctx.body_reader.bytes().to_bytes()

        assert type(ctx.body_reader) is SyncResponseBodyReader
        assert type(ctx.headers) is HeaderMap
        assert type(ctx.extensions) is dict

        return {**json.loads(content), "test": "bar"}

    with SyncClientBuilder().json_handler(loads=custom_loads).error_for_status(True).build() as client:
        resp = client.get(echo_server.url).extensions({"my_ext": "foo"}).build().send()
        assert called == 0
        res = resp.json()
        assert called == 1
        assert res.pop("test") == "bar"
        assert json.loads((resp.bytes()).to_bytes()) == res
        assert resp.json() == {**res, "test": "bar"}
        assert called == 2

    async def bad_loads(_ctx: SyncJsonLoadsContext) -> Any:
        raise RuntimeError("should not be called")

    with pytest.raises(ValueError, match="loads must be a sync function"):
        SyncClientBuilder().json_handler(loads=bad_loads)

    with pytest.raises(ValueError, match="Expected a callable"):
        SyncClientBuilder().json_handler(loads="bad")  # type: ignore[arg-type]


def test_use_after_close(echo_server: SubprocessServer):
    with client_builder().build() as client:
        assert client.get(echo_server.url).build().send().status == 200
    req = client.get(echo_server.url).build()
    with pytest.raises(ClientClosedError, match="Client was closed"):
        req.send()

    client = client_builder().build()
    client.close()
    req = client.get(echo_server.url).build()
    with pytest.raises(ClientClosedError, match="Client was closed"):
        req.send()


def test_stream_use_after_close(client: SyncClient, echo_body_parts_server: SubprocessServer):
    def stream_gen() -> Iterator[bytes]:
        yield b"part 0"
        yield b"part 1"

    req = client.post(echo_body_parts_server.url).body_stream(stream_gen()).build_streamed()

    with req as resp:
        assert resp.body_reader.read_chunk() == b"part 0"

    with pytest.raises(RuntimeError, match="Response body reader is closed"):
        _ = resp.body_reader
    with pytest.raises(RuntimeError, match="Response body reader is closed"):
        resp.json()
    with pytest.raises(RuntimeError, match="Response body reader is closed"):
        resp.text()
    with pytest.raises(RuntimeError, match="Response body reader is closed"):
        resp.bytes()
    with pytest.raises(RuntimeError, match="Response body reader is closed"):
        resp.body_reader.read(100)
    assert resp.headers["content-type"] == "application/json"


def test_types(echo_server: SubprocessServer) -> None:
    builder = SyncClientBuilder().error_for_status(True)
    assert type(builder) is SyncClientBuilder and isinstance(builder, BaseClientBuilder)
    client = builder.build()
    assert type(client) is SyncClient and isinstance(client, BaseClient)
    req_builder = client.get(echo_server.url)
    assert type(req_builder) is SyncRequestBuilder and isinstance(req_builder, BaseRequestBuilder)
    req = req_builder.build()
    assert type(req) is SyncConsumedRequest and isinstance(req, Request)
    resp = req.send()
    assert type(resp) is SyncResponse and isinstance(resp, BaseResponse)
