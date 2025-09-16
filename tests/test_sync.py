import json
from collections.abc import Generator
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
from pyreqwest.request import BaseRequestBuilder, Request, SyncConsumedRequest, SyncRequestBuilder
from pyreqwest.response import BaseResponse, SyncResponse, SyncResponseBodyReader

from tests.servers.server import Server

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


def test_send(client: SyncClient, echo_server: Server) -> None:
    assert client.get(echo_server.url).build().send().json()["method"] == "GET"


def test_middleware(echo_server: Server) -> None:
    def middleware(request: Request, next_handler: SyncNext) -> SyncResponse:
        request.headers["x-test1"] = "foo"
        response = next_handler.run(request)
        response.headers["x-test2"] = "bar"
        return response

    with middleware_client(middleware) as client:
        resp = client.get(echo_server.url).build().send()
        assert ["x-test1", "foo"] in resp.json()["headers"]
        assert resp.headers["x-test2"] == "bar"


def test_context_vars(echo_server: Server) -> None:
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


def test_stream(client: SyncClient, echo_body_parts_server: Server) -> None:
    def gen() -> Generator[bytes, None, None]:
        for i in range(3):
            yield f"part {i}".encode()

    with client.post(echo_body_parts_server.url).body_stream(gen()).build_streamed() as resp:
        assert resp.body_reader.read_chunk() == b"part 0"
        assert resp.body_reader.read_chunk() == b"part 1"
        assert resp.body_reader.read_chunk() == b"part 2"
        assert resp.body_reader.read_chunk() is None


@pytest.mark.parametrize("concurrency", [1, 2, 10])
@pytest.mark.parametrize("limit", [None, 1, 2, 10])
def test_concurrent_requests(echo_server: Server, concurrency: int, limit: int | None) -> None:
    builder = client_builder()
    if limit is not None:
        builder = builder.max_connections(limit)

    with builder.build() as client, ThreadPoolExecutor(max_workers=10) as executor:
        futures = [
            executor.submit(lambda: client.get(echo_server.url).build().send().json()) for _ in range(concurrency)
        ]
        assert all(fut.result()["method"] == "GET" for fut in futures)


@pytest.mark.parametrize("max_conn", [1, 2, None])
def test_max_connections_pool_timeout(echo_server: Server, max_conn: int | None):
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


def test_json_loads_callback(echo_server: Server):
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


def test_use_after_close(echo_server: Server):
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


def test_types(echo_server: Server) -> None:
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
