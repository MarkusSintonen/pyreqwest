from collections.abc import Generator
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from datetime import timedelta
from typing import TypeVar

import pytest
from pyreqwest.client import BaseClient, BaseClientBuilder, BlockingClient, BlockingClientBuilder
from pyreqwest.exceptions import ClientClosedError, PoolTimeoutError
from pyreqwest.middleware import BlockingNext
from pyreqwest.middleware.types import BlockingMiddleware
from pyreqwest.request import BaseRequestBuilder, BlockingConsumedRequest, BlockingRequestBuilder, Request
from pyreqwest.response import BaseResponse, BlockingResponse

from tests.servers.server import Server

T = TypeVar("T")


def client_builder() -> BlockingClientBuilder:
    return BlockingClientBuilder().error_for_status(True).timeout(timedelta(seconds=5))


@contextmanager
def middleware_client(middleware: BlockingMiddleware) -> Generator[BlockingClient, None, None]:
    with client_builder().with_middleware(middleware).build() as client:
        yield client


@pytest.fixture
def client() -> Generator[BlockingClient, None, None]:
    with client_builder().build() as client:
        yield client


def test_send(client: BlockingClient, echo_server: Server) -> None:
    req = client.get(echo_server.url).build_consumed()
    assert req.send().json()["method"] == "GET"


def test_middleware(echo_server: Server) -> None:
    def middleware(request: Request, next_: BlockingNext) -> BlockingResponse:
        request.headers["x-test"] = "foo"
        return next_.run(req)

    with middleware_client(middleware) as client:
        req = client.get(echo_server.url).build_consumed()
        assert ["x-test", "foo"] in req.send().json()["headers"]


def test_stream(client: BlockingClient, echo_body_parts_server: Server) -> None:
    def gen() -> Generator[bytes, None, None]:
        for i in range(3):
            yield f"part {i}".encode()

    with client.post(echo_body_parts_server.url).body_stream(gen()).build_streamed() as resp:
        assert resp.next_chunk() == b"part 0"
        assert resp.next_chunk() == b"part 1"
        assert resp.next_chunk() == b"part 2"
        assert resp.next_chunk() is None


@pytest.mark.parametrize("concurrency", [1, 2, 10])
@pytest.mark.parametrize("limit", [None, 1, 2, 10])
def test_concurrent_requests(echo_server: Server, concurrency: int, limit: int | None) -> None:
    builder = client_builder()
    if limit is not None:
        builder = builder.max_connections(limit)

    with builder.build() as client, ThreadPoolExecutor(max_workers=10) as executor:
        futures = [
            executor.submit(lambda: client.get(echo_server.url).build_consumed().send().json())
            for _ in range(concurrency)
        ]
        assert all(fut.result()["method"] == "GET" for fut in futures)


@pytest.mark.parametrize("value", [1, 2, None])
def test_max_connections_pool_timeout(echo_server: Server, value: int | None):
    url = echo_server.url.with_query({"sleep_start": 0.1})

    builder = client_builder().max_connections(value).pool_timeout(timedelta(seconds=0.05))

    with builder.build() as client, ThreadPoolExecutor(max_workers=10) as executor:
        futures = [executor.submit(lambda: client.get(url).build_consumed().send().json()) for _ in range(2)]
        if value == 1:
            with pytest.raises(PoolTimeoutError) as e:
                _ = [fut.result() for fut in futures]
            assert isinstance(e.value, TimeoutError)
        else:
            assert all(fut.result()["method"] == "GET" for fut in futures)


def test_use_after_close(echo_server: Server):
    with client_builder().build() as client:
        assert client.get(echo_server.url).build_consumed().send().status == 200
    req = client.get(echo_server.url).build_consumed()
    with pytest.raises(ClientClosedError, match="Client was closed"):
        req.send()

    client = client_builder().build()
    client.close()
    req = client.get(echo_server.url).build_consumed()
    with pytest.raises(ClientClosedError, match="Client was closed"):
        req.send()


def test_types(echo_server: Server) -> None:
    builder = BlockingClientBuilder().error_for_status(True)
    assert type(builder) is BlockingClientBuilder and isinstance(builder, BaseClientBuilder)
    client = builder.build()
    assert type(client) is BlockingClient and isinstance(client, BaseClient)
    req_builder = client.get(echo_server.url)
    assert type(req_builder) is BlockingRequestBuilder and isinstance(req_builder, BaseRequestBuilder)
    req = req_builder.build_consumed()
    assert type(req) is BlockingConsumedRequest and isinstance(req, Request)
    resp = req.send()
    assert type(resp) is BlockingResponse and isinstance(resp, BaseResponse)
