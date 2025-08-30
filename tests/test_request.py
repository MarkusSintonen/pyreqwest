import asyncio
import copy
import gc
import time
import weakref
from collections.abc import AsyncGenerator
from datetime import timedelta
from typing import Any

import pytest
import trustme
from pyreqwest.client import Client, ClientBuilder
from pyreqwest.http import Body, HeaderMap
from pyreqwest.request import ConsumedRequest, Request, StreamRequest
from pyreqwest.types import Stream
from syrupy import SnapshotAssertion  # type: ignore[attr-defined]

from .servers.server import Server


@pytest.fixture
async def client(cert_authority: trustme.CA) -> AsyncGenerator[Client, None]:
    cert_pem = cert_authority.cert_pem.bytes()
    async with ClientBuilder().error_for_status(True).add_root_certificate_pem(cert_pem).build() as client:
        yield client


async def test_method(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).build_consumed()
    assert req.method == "GET"
    req.method = "POST"
    resp = await req.send()
    assert (await resp.json())["method"] == "POST"


async def test_url(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).query({"a": "b"}).build_consumed()
    assert req.url == echo_server.url.with_query({"a": "b"})
    req.url = req.url.with_query({"test": "value"})
    assert req.url.query_pairs == [("test", "value")]


async def test_headers(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).headers({"X-Test1": "Value1", "X-Test2": "Value2"}).build_consumed()
    assert req.headers["X-Test1"] == "Value1" and req.headers["x-test1"] == "Value1"
    assert req.headers["X-Test2"] == "Value2" and req.headers["x-test2"] == "Value2"

    req.headers["X-Test3"] = "Value3"
    assert req.headers["X-Test3"] == "Value3" and req.headers["x-test3"] == "Value3"

    assert req.headers.pop("x-test1")
    assert "X-Test1" not in req.headers and "x-test1" not in req.headers

    resp = await req.send()
    assert sorted([(k, v) for k, v in (await resp.json())["headers"] if k.startswith("x-")]) == [
        ("x-test2", "Value2"),
        ("x-test3", "Value3"),
    ]


@pytest.mark.parametrize("kind", ["bytes", "text"])
async def test_body__content(client: Client, echo_server: Server, kind: str) -> None:
    def body() -> Body:
        if kind == "bytes":
            return Body.from_bytes(b"test1")
        assert kind == "text"
        return Body.from_text("test1")

    req = client.post(echo_server.url).build_consumed()
    assert req.body is None
    req.body = body()
    assert req.body is not None and req.body.copy_bytes() == b"test1" and req.body.get_stream() is None
    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test1"]

    req = client.post(echo_server.url).body_bytes(b"test2").build_consumed()
    assert req.body is not None and req.body.copy_bytes() == b"test2" and req.body.get_stream() is None
    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test2"]

    resp = await client.post(echo_server.url).body_bytes(b"test3").build_consumed().send()
    assert (await resp.json())["body_parts"] == ["test3"]


@pytest.mark.parametrize("yield_type", [bytes, bytearray, memoryview])
async def test_body__stream_fn(
    client: Client,
    echo_server: Server,
    yield_type: type[bytes] | type[bytearray] | type[memoryview],
) -> None:
    async def stream_gen() -> Stream:
        yield yield_type(b"test1")
        yield yield_type(b"test2")

    stream = stream_gen()
    req = client.post(echo_server.url).build_consumed()
    assert req.body is None
    req.body = Body.from_stream(stream)
    assert req.body is not None and req.body.get_stream() is stream and req.body.copy_bytes() is None
    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test1", "test2"]

    stream = stream_gen()
    req = client.post(echo_server.url).body_stream(stream).build_consumed()
    assert req.body is not None and req.body.get_stream() is stream and req.body.copy_bytes() is None
    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test1", "test2"]

    stream = stream_gen()
    resp = await client.post(echo_server.url).body_stream(stream).build_consumed().send()
    assert (await resp.json())["body_parts"] == ["test1", "test2"]


async def test_body__stream_class(client: Client, echo_server: Server) -> None:
    class StreamGen:
        def __aiter__(self) -> AsyncGenerator[bytes]:
            async def gen() -> AsyncGenerator[bytes]:
                yield b"test1"
                yield b"test2"

            return gen()

    stream = StreamGen()
    req = client.post(echo_server.url).build_consumed()
    assert req.body is None
    req.body = Body.from_stream(stream)
    assert req.body is not None and req.body.get_stream() is stream and req.body.copy_bytes() is None
    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test1", "test2"]

    stream = StreamGen()
    req = client.post(echo_server.url).body_stream(stream).build_consumed()
    assert req.body is not None and req.body.get_stream() is stream and req.body.copy_bytes() is None
    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test1", "test2"]

    stream = StreamGen()
    resp = await client.post(echo_server.url).body_stream(stream).build_consumed().send()
    assert (await resp.json())["body_parts"] == ["test1", "test2"]


async def test_body__stream_error_already_used(client: Client, echo_server: Server) -> None:
    async def stream_gen() -> AsyncGenerator[bytes]:
        yield b"test1"

    body = Body.from_stream(stream_gen())
    req = client.post(echo_server.url).build_consumed()
    req.body = body
    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test1"]

    req = client.post(echo_server.url).build_consumed()
    req.body = body
    with pytest.raises(RuntimeError, match="Body already consumed"):
        await req.send()


async def test_extensions(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).extensions({"a": "b"}).build_consumed()
    assert req.extensions == {"a": "b"}
    req.extensions = {"foo": "bar", "test": "value"}
    assert req.extensions.pop("test") == "value"
    assert req.extensions == {"foo": "bar"}


@pytest.mark.parametrize("call", ["copy", "__copy__"])
@pytest.mark.parametrize("build", ["consumed", "streamed"])
async def test_copy(client: Client, echo_server: Server, call: str, build: str) -> None:
    builder = client.get(echo_server.url).body_text("test1").header("X-Test1", "Val1")
    if build == "consumed":
        req1: Request = builder.build_consumed()
    else:
        assert build == "streamed"
        req1 = builder.build_streamed()

    if call == "copy":
        req2 = req1.copy()
    else:
        assert call == "__copy__"
        req2 = copy.copy(req1)

    assert req1.method == req2.method == "GET"
    assert req1.url == req2.url
    assert req1.headers["x-test1"] == req2.headers["x-test1"] == "Val1"
    assert req1.body and req2.body and req1.body.copy_bytes() == req2.body.copy_bytes() == b"test1"

    if build == "consumed":
        assert isinstance(req1, ConsumedRequest) and isinstance(req2, ConsumedRequest)
        resp1 = await req1.send()
        resp2 = await req2.send()
        assert (await resp1.json()) == (await resp2.json())
    else:
        assert build == "streamed"
        assert isinstance(req1, StreamRequest) and isinstance(req2, StreamRequest)
        async with req1 as resp1, req2 as resp2:
            assert (await resp1.json()) == (await resp2.json())


async def test_duplicate_send_fails(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).build_consumed()
    await req.send()
    with pytest.raises(RuntimeError, match="Request was already sent"):
        await req.send()


async def test_duplicate_context_manager_fails(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).build_streamed()
    async with req as _:
        pass
    with pytest.raises(RuntimeError, match="Request was already sent"):
        async with req as _:
            pytest.fail("Should not get here")

    req = client.get(echo_server.url).build_streamed()
    async with req as _:
        with pytest.raises(RuntimeError, match="Request was already sent"):
            async with req as _:
                pytest.fail("Should not get here")


async def test_cancel(client: Client, echo_server: Server) -> None:
    request = client.get(echo_server.url.with_query({"sleep_start": 5})).build_consumed()

    task = asyncio.create_task(request.send())
    start = time.time()
    await asyncio.sleep(0.5)  # Allow the request to start processing
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task
    assert time.time() - start < 1


@pytest.mark.parametrize("sleep_in", ["stream_gen", "server"])
async def test_cancel_stream_request(client: Client, echo_body_parts_server: Server, sleep_in: str) -> None:
    async def stream_gen() -> AsyncGenerator[bytes]:
        if sleep_in == "stream_gen":
            yield b"test1"
            await asyncio.sleep(5)
        else:
            assert sleep_in == "server"
            yield b"test1"
            yield b'{"sleep": 5}'
        yield b"test2"

    request = client.post(echo_body_parts_server.url).body_stream(stream_gen()).build_streamed()

    async def run_request(req: StreamRequest) -> None:
        async with req as _:
            pytest.fail("Request should have been cancelled")

    task = asyncio.create_task(run_request(request))
    start = time.time()
    await asyncio.sleep(0.5)  # Allow the request to start processing
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task
    assert time.time() - start < 1


class StreamRepr:
    def __aiter__(self) -> AsyncGenerator[bytes]:
        async def gen() -> AsyncGenerator[bytes]:
            yield b"test"

        return gen()

    def __repr__(self) -> str:
        return "StreamRepr()"


def test_repr(snapshot: SnapshotAssertion):
    client = ClientBuilder().build()
    url = "https://example.com/test?foo=bar"
    headers = HeaderMap({"X-Test": "Value"})
    headers.append("X-Another", "AnotherValue", is_sensitive=True)
    req = client.get(url).headers(headers).build_consumed()
    assert repr(req) == snapshot(name="repr_sensitive")
    assert req.repr_full() == snapshot(name="repr_full")

    req = client.get("https://example.com").body(Body.from_text("test")).build_consumed()
    assert repr(req) == snapshot(name="repr_body")
    assert req.repr_full() == snapshot(name="repr_full_body")

    req = client.get("https://example.com").body(Body.from_stream(StreamRepr())).build_consumed()
    assert repr(req) == snapshot(name="repr_stream_body")
    assert req.repr_full() == snapshot(name="repr_full_stream_body")

    streamed = client.get("https://example.com").body(Body.from_stream(StreamRepr())).build_streamed()
    assert repr(streamed) == repr(req)
    assert streamed.repr_full() == req.repr_full()


def test_circular_reference_collected(echo_server: Server) -> None:
    # Check the GC support via __traverse__ and __clear__
    ref: weakref.ReferenceType[Any] | None = None

    def check() -> None:
        nonlocal ref

        class StreamHandler:
            def __init__(self) -> None:
                self.request: Request | None = None

            def __aiter__(self) -> AsyncGenerator[bytes]:
                async def gen() -> AsyncGenerator[bytes]:
                    yield b"test"

                return gen()

        client = ClientBuilder().error_for_status(True).timeout(timedelta(seconds=5)).build()

        stream = StreamHandler()
        ref = weakref.ref(stream)
        request = client.post(echo_server.url).body(Body.from_stream(stream)).build_consumed()
        stream.request = request

    check()
    gc.collect()
    assert ref is not None and ref() is None
