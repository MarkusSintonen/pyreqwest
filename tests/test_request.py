import asyncio
import copy
import gc
import json
import time
import weakref
from collections.abc import AsyncGenerator
from datetime import timedelta
from typing import Any

import pytest
import trustme
from pyreqwest.client import Client, ClientBuilder
from pyreqwest.client.types import JsonLoadsContext
from pyreqwest.http import HeaderMap
from pyreqwest.request import ConsumedRequest, Request, RequestBody, StreamRequest
from pyreqwest.types import Stream
from syrupy import SnapshotAssertion  # type: ignore[attr-defined]

from tests.servers.server_subprocess import SubprocessServer


@pytest.fixture
async def client(cert_authority: trustme.CA) -> AsyncGenerator[Client, None]:
    cert_pem = cert_authority.cert_pem.bytes()
    async with ClientBuilder().error_for_status(True).add_root_certificate_pem(cert_pem).build() as client:
        yield client


async def test_method(client: Client, echo_server: SubprocessServer) -> None:
    req = client.get(echo_server.url).build()
    assert req.method == "GET"
    req.method = "POST"
    resp = await req.send()
    assert (await resp.json())["method"] == "POST"


async def test_url(client: Client, echo_server: SubprocessServer) -> None:
    req = client.get(echo_server.url).query({"a": "b"}).build()
    assert req.url == echo_server.url.with_query({"a": "b"})
    req.url = req.url.with_query({"test": "value"})
    assert req.url.query_pairs == [("test", "value")]


async def test_headers(client: Client, echo_server: SubprocessServer) -> None:
    req = client.get(echo_server.url).headers({"X-Test1": "Value1", "X-Test2": "Value2"}).build()
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


async def test_headers__get_set(client: Client, echo_server: SubprocessServer) -> None:
    req = client.get(echo_server.url).build()
    assert isinstance(req.headers, HeaderMap) and req.headers == {}
    req.headers = {"X-Test1": "Value1"}
    assert isinstance(req.headers, HeaderMap) and req.headers == {"x-test1": "Value1"}
    req2 = req.copy()
    req3 = req.copy()
    assert isinstance(req2.headers, HeaderMap) and req2.headers == {"x-test1": "Value1"}
    req2.headers = [("X-Test2", "Value2"), ("X-Test2", "Value3")]
    assert isinstance(req2.headers, HeaderMap) and req2.headers == HeaderMap(
        [("x-test2", "Value2"), ("x-test2", "Value3")]
    )

    resp = await req.send()
    assert [(k, v) for k, v in (await resp.json())["headers"] if k.startswith("x-")] == [("x-test1", "Value1")]
    resp2 = await req2.send()
    assert [(k, v) for k, v in (await resp2.json())["headers"] if k.startswith("x-")] == [
        ("x-test2", "Value2"),
        ("x-test2", "Value3"),
    ]
    resp3 = await req3.send()
    assert [(k, v) for k, v in (await resp3.json())["headers"] if k.startswith("x-")] == [("x-test1", "Value1")]


async def test_headers__client_default(echo_server: SubprocessServer) -> None:
    async with ClientBuilder().error_for_status(True).default_headers({"X-Default": "Value1"}).build() as client:
        req = client.get(echo_server.url).build()
        req2 = req.copy()
        assert req.headers == {"X-Default": "Value1"}
        req.headers["X-Test"] = "Value2"
        assert req.headers == {"X-Default": "Value1", "X-Test": "Value2"}

        resp = await req.send()
        assert [(k, v) for k, v in (await resp.json())["headers"] if k.startswith("x-")] == [
            ("x-default", "Value1"),
            ("x-test", "Value2"),
        ]
        resp2 = await req2.send()
        assert [(k, v) for k, v in (await resp2.json())["headers"] if k.startswith("x-")] == [("x-default", "Value1")]


@pytest.mark.parametrize("kind", ["bytes", "text"])
async def test_body__content(client: Client, echo_server: SubprocessServer, kind: str) -> None:
    def body() -> RequestBody:
        if kind == "bytes":
            return RequestBody.from_bytes(b"test1")
        assert kind == "text"
        return RequestBody.from_text("test1")

    req = client.post(echo_server.url).build()
    assert req.body is None
    req.body = body()
    assert req.body is not None and req.body.copy_bytes() == b"test1" and req.body.get_stream() is None
    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test1"]

    req = client.post(echo_server.url).body_bytes(b"test2").build()
    assert req.body is not None and req.body.copy_bytes() == b"test2" and req.body.get_stream() is None
    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test2"]

    resp = await client.post(echo_server.url).body_bytes(b"test3").build().send()
    assert (await resp.json())["body_parts"] == ["test3"]


@pytest.mark.parametrize("yield_type", [bytes, bytearray, memoryview])
async def test_body__stream_fn(
    client: Client,
    echo_server: SubprocessServer,
    yield_type: type[bytes] | type[bytearray] | type[memoryview],
) -> None:
    async def stream_gen() -> Stream:
        yield yield_type(b"test1")
        yield yield_type(b"test2")

    stream = stream_gen()
    req = client.post(echo_server.url).build()
    assert req.body is None
    req.body = RequestBody.from_stream(stream)
    assert req.body is not None and req.body.get_stream() is stream and req.body.copy_bytes() is None
    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test1", "test2"]

    stream = stream_gen()
    req = client.post(echo_server.url).body_stream(stream).build()
    assert req.body is not None and req.body.get_stream() is stream and req.body.copy_bytes() is None
    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test1", "test2"]

    stream = stream_gen()
    resp = await client.post(echo_server.url).body_stream(stream).build().send()
    assert (await resp.json())["body_parts"] == ["test1", "test2"]


async def test_body__stream_class(client: Client, echo_server: SubprocessServer) -> None:
    class StreamGen:
        def __aiter__(self) -> AsyncGenerator[bytes]:
            async def gen() -> AsyncGenerator[bytes]:
                yield b"test1"
                yield b"test2"

            return gen()

    stream = StreamGen()
    req = client.post(echo_server.url).build()
    assert req.body is None
    req.body = RequestBody.from_stream(stream)
    assert req.body is not None and req.body.get_stream() is stream and req.body.copy_bytes() is None
    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test1", "test2"]

    stream = StreamGen()
    req = client.post(echo_server.url).body_stream(stream).build()
    assert req.body is not None and req.body.get_stream() is stream and req.body.copy_bytes() is None
    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test1", "test2"]

    stream = StreamGen()
    resp = await client.post(echo_server.url).body_stream(stream).build().send()
    assert (await resp.json())["body_parts"] == ["test1", "test2"]


async def test_body__stream_error(client: Client, echo_server: SubprocessServer) -> None:
    class StreamGen:
        def __aiter__(self) -> AsyncGenerator[bytes]:
            raise TypeError("test error")

    with pytest.raises(TypeError, match="test error"):
        client.post(echo_server.url).body_stream(StreamGen())

    class StreamGen2:
        def __aiter__(self) -> AsyncGenerator[bytes]:
            async def gen() -> AsyncGenerator[bytes]:
                yield b"test"
                raise TypeError("test error")

            return gen()

    req = client.post(echo_server.url).body_stream(StreamGen2()).build()
    with pytest.raises(TypeError, match="test error"):
        await req.send()


async def test_body__stream_error_already_used(client: Client, echo_server: SubprocessServer) -> None:
    async def stream_gen() -> AsyncGenerator[bytes]:
        yield b"test1"

    body = RequestBody.from_stream(stream_gen())
    req = client.post(echo_server.url).build()
    req.body = body
    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test1"]

    req = client.post(echo_server.url).build()
    req.body = body
    with pytest.raises(RuntimeError, match="Request body already consumed"):
        await req.send()
    with pytest.raises(RuntimeError, match="Request body already consumed"):
        _ = body.copy_bytes()
    with pytest.raises(RuntimeError, match="Request body already consumed"):
        _ = body.get_stream()
    with pytest.raises(RuntimeError, match="Request body already consumed"):
        copy.copy(body)

    req = client.post(echo_server.url).build()
    req.body = body
    with pytest.raises(RuntimeError, match="Request body already consumed"):
        await req.send()


async def test_body__get_set(client: Client, echo_server: SubprocessServer) -> None:
    req = client.get(echo_server.url).build()
    assert req.body is None
    req.body = RequestBody.from_bytes(b"test1")
    assert req.body is not None and req.body.copy_bytes() == b"test1" and req.body.get_stream() is None
    req2 = req.copy()
    req3 = req.copy()
    assert req2.body is not None and req2.body.copy_bytes() == b"test1" and req2.body.get_stream() is None
    req2.body = RequestBody.from_bytes(b"test2")
    assert req2.body is not None and req2.body.copy_bytes() == b"test2" and req2.body.get_stream() is None

    resp = await req.send()
    assert (await resp.json())["body_parts"] == ["test1"]
    resp2 = await req2.send()
    assert (await resp2.json())["body_parts"] == ["test2"]
    resp3 = await req3.send()
    assert (await resp3.json())["body_parts"] == ["test1"]


async def test_extensions(client: Client, echo_server: SubprocessServer) -> None:
    req = client.get(echo_server.url).extensions({"a": "b"}).build()
    assert req.extensions == {"a": "b"}
    req.extensions = {"foo": "bar", "test": "value"}
    assert req.extensions.pop("test") == "value"
    assert req.extensions == {"foo": "bar"}


@pytest.mark.parametrize("call", ["copy", "__copy__"])
@pytest.mark.parametrize("build_streamed", [False, True])
@pytest.mark.parametrize("body_streamed", [False, True])
async def test_copy(echo_server: SubprocessServer, call: str, build_streamed: bool, body_streamed: bool) -> None:
    async def remove_time(ctx: JsonLoadsContext) -> Any:
        d = json.loads((await ctx.body_reader.bytes()).to_bytes())
        d.pop("time")
        return d

    client = ClientBuilder().json_handler(loads=remove_time).error_for_status(True).build()

    builder = client.get(echo_server.url).header("X-Test1", "Val1")
    stream_copied = False

    if body_streamed:

        class StreamGen:
            def __aiter__(self) -> AsyncGenerator[bytes]:
                async def gen() -> AsyncGenerator[bytes]:
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
        assert isinstance(req1, StreamRequest) and isinstance(req2, StreamRequest)
        async with req1 as resp1, req2 as resp2:
            assert (await resp1.json()) == (await resp2.json())
    else:
        assert isinstance(req1, ConsumedRequest) and isinstance(req2, ConsumedRequest)
        resp1 = await req1.send()
        resp2 = await req2.send()
        assert (await resp1.json()) == (await resp2.json())


async def test_duplicate_send_fails(client: Client, echo_server: SubprocessServer) -> None:
    req = client.get(echo_server.url).build()
    await req.send()
    with pytest.raises(RuntimeError, match="Request was already sent"):
        await req.send()


async def test_duplicate_context_manager_fails(client: Client, echo_server: SubprocessServer) -> None:
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


async def test_cancel(client: Client, echo_server: SubprocessServer) -> None:
    request = client.get(echo_server.url.with_query({"sleep_start": 5})).build()

    task = asyncio.create_task(request.send())
    start = time.time()
    await asyncio.sleep(0.5)  # Allow the request to start processing
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task
    assert time.time() - start < 1


@pytest.mark.parametrize("sleep_in", ["stream_gen", "server"])
async def test_cancel_stream_request(client: Client, echo_body_parts_server: SubprocessServer, sleep_in: str) -> None:
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


async def test_use_after_send(client: Client, echo_server: SubprocessServer) -> None:
    req = client.get(echo_server.url).build()
    await req.send()
    with pytest.raises(RuntimeError, match="Request was already sent"):
        _ = req.method
    with pytest.raises(RuntimeError, match="Request was already sent"):
        _ = req.url
    with pytest.raises(RuntimeError, match="Request was already sent"):
        _ = req.headers
    with pytest.raises(RuntimeError, match="Request was already sent"):
        _ = req.body
    with pytest.raises(RuntimeError, match="Request was already sent"):
        _ = req.extensions


class StreamRepr:
    def __aiter__(self) -> AsyncGenerator[bytes]:
        async def gen() -> AsyncGenerator[bytes]:
            yield b"test"

        return gen()

    def __repr__(self) -> str:
        return "StreamRepr()"


async def test_repr(snapshot: SnapshotAssertion, echo_server: SubprocessServer) -> None:
    client = ClientBuilder().build()
    url = "https://example.com/test?foo=bar"
    headers = HeaderMap({"X-Test": "Value"})
    headers.append("X-Another", "AnotherValue", is_sensitive=True)
    req1 = client.get(url).headers(headers).build()
    assert repr(req1) == snapshot(name="repr_sensitive")
    assert req1.repr_full() == snapshot(name="repr_full")

    req2 = client.get("https://example.com").body_text("test").build()
    assert repr(req2) == snapshot(name="repr_body")
    assert req2.repr_full() == snapshot(name="repr_full_body")

    req3 = client.get("https://example.com").body_stream(StreamRepr()).build()
    assert repr(req3) == snapshot(name="repr_stream_body")
    assert req3.repr_full() == snapshot(name="repr_full_stream_body")

    req4 = client.get("https://example.com").build()
    req4.body = RequestBody.from_stream(StreamRepr())
    assert repr(req4) == snapshot(name="repr_set_stream_body")
    assert req4.repr_full() == snapshot(name="repr_set_full_stream_body")

    streamed = client.get("https://example.com").body_stream(StreamRepr()).build_streamed()
    assert repr(streamed) == repr(req3)
    assert streamed.repr_full() == req3.repr_full()

    sent_req = client.get(echo_server.url).body_stream(StreamRepr()).build()
    assert repr(sent_req.body) == "RequestBody(stream=StreamRepr())"
    body = sent_req.body
    await sent_req.send()
    assert repr(body) == "RequestBody(<already consumed>)"


def test_consumed_request_read_buffer_limit_fails(client: Client, echo_server: SubprocessServer) -> None:
    req = client.get(echo_server.url).build()
    with pytest.raises(RuntimeError, match="Expected streamed request, found fully consumed request"):
        _ = req.read_buffer_limit  # type: ignore[attr-defined]


def test_circular_reference_collected(echo_server: SubprocessServer) -> None:
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
        request = client.post(echo_server.url).body_stream(stream).build()
        stream.request = request

    check()
    gc.collect()
    assert ref is not None and ref() is None
