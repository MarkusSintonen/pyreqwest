import asyncio
import traceback
from collections.abc import AsyncGenerator, AsyncIterator, Generator
from datetime import timedelta
from typing import Any

import orjson
import pytest
from pyreqwest.client import Client, ClientBuilder
from pyreqwest.exceptions import ConnectTimeoutError, ReadTimeoutError
from pyreqwest.request import RequestBuilder
from pyreqwest.response import Response
from pyreqwest.types import Stream

from .servers.echo_body_parts_server import EchoBodyPartsServer
from .servers.echo_server import EchoServer


@pytest.fixture
async def client() -> AsyncGenerator[Client, None]:
    async with ClientBuilder().error_for_status(True).build() as client:
        yield client


async def read_chunks(resp: Response):
    while (chunk := await resp.body_reader.read_chunk()) is not None:
        yield chunk


@pytest.mark.parametrize("read_buffer_limit", [None, 0, 10, 999999])
@pytest.mark.parametrize("read", ["chunks", "bytes", "text"])
@pytest.mark.parametrize("yield_empty", [False, True])
async def test_body_stream__read_buffer_limit(
    client: Client,
    echo_body_parts_server: EchoBodyPartsServer,
    read_buffer_limit: int | None,
    read: str,
    yield_empty: bool,
):
    async def stream_gen() -> AsyncGenerator[bytes]:
        for i in range(5):
            await asyncio.sleep(0)  # Simulate some work
            if yield_empty and i == 2:
                yield b""  # Empty is skipped
            else:
                yield f"part {i}".encode()

    req_builder = client.post(echo_body_parts_server.url).body_stream(stream_gen())
    if read_buffer_limit is not None:
        req_builder = req_builder.streamed_read_buffer_limit(read_buffer_limit)

    req = req_builder.build_streamed()

    if read_buffer_limit is not None:
        assert req.read_buffer_limit == read_buffer_limit
    else:
        assert req.read_buffer_limit == RequestBuilder.default_streamed_read_buffer_limit()

    expected = [b"part 0", b"part 1", b"part 2", b"part 3", b"part 4"]
    if yield_empty:
        expected.remove(b"part 2")

    async with req as resp:
        if read == "chunks":
            assert [c async for c in read_chunks(resp)] == expected
        elif read == "bytes":
            assert (await resp.bytes()) == b"".join(expected)
            assert (await resp.bytes()) == b"".join(expected)
        else:
            assert read == "text"
            assert (await resp.text()) == "".join([c.decode("utf-8") for c in expected])
            assert (await resp.text()) == "".join([c.decode("utf-8") for c in expected])


@pytest.mark.parametrize("read", ["chunks", "bytes", "text"])
async def test_body_stream__consumed(client: Client, echo_body_parts_server: EchoBodyPartsServer, read: str):
    async def stream_gen() -> AsyncGenerator[bytes]:
        for i in range(5):
            await asyncio.sleep(0)  # Simulate some work
            yield f"part {i}".encode()

    resp = await client.post(echo_body_parts_server.url).body_stream(stream_gen()).build().send()
    if read == "chunks":
        assert [c async for c in read_chunks(resp)] == [b"part 0", b"part 1", b"part 2", b"part 3", b"part 4"]
    elif read == "bytes":
        assert (await resp.bytes()) == b"part 0part 1part 2part 3part 4"
        assert (await resp.bytes()) == b"part 0part 1part 2part 3part 4"
    else:
        assert read == "text"
        assert (await resp.text()) == "part 0part 1part 2part 3part 4"
        assert (await resp.text()) == "part 0part 1part 2part 3part 4"


@pytest.mark.parametrize("gen_type", ["async", "sync", "list"])
@pytest.mark.parametrize("yield_type", [bytes, bytearray, memoryview])
async def test_body_stream__gen_type(
    client: Client, echo_body_parts_server: EchoBodyPartsServer, gen_type: str, yield_type: type
):
    if gen_type == "async":

        async def async_gen() -> AsyncIterator[Any]:
            for i in range(5):
                await asyncio.sleep(0)  # Simulate some work
                yield yield_type(f"part {i}".encode())

        stream: Stream = async_gen()
    elif gen_type == "sync":

        def gen() -> Generator[Any, None, None]:
            for i in range(5):
                yield yield_type(f"part {i}".encode())

        stream = gen()
    else:
        assert gen_type == "list"
        stream = [yield_type(f"part {i}".encode()) for i in range(5)]

    async with client.post(echo_body_parts_server.url).body_stream(stream).build_streamed() as resp:
        assert [c async for c in read_chunks(resp)] == [b"part 0", b"part 1", b"part 2", b"part 3", b"part 4"]


@pytest.mark.parametrize("yield_val", ["bad", [b"a"], None])
async def test_body_stream__bad_yield_type(client: Client, echo_body_parts_server: EchoBodyPartsServer, yield_val: Any):
    async def stream_gen() -> AsyncGenerator[Any]:
        yield yield_val

    req = client.post(echo_body_parts_server.url).body_stream(stream_gen()).build_streamed()

    with pytest.raises(TypeError, match="a bytes-like object is required"):
        async with req as _:
            pytest.fail("Should have raised")


@pytest.mark.parametrize("read_buffer_limit", [None, 0, 5, 999999])
@pytest.mark.parametrize("sleep_kind", ["server", "stream"])
async def test_body_stream__timeout(
    client: Client,
    echo_body_parts_server: EchoBodyPartsServer,
    read_buffer_limit: int | None,
    sleep_kind: str,
):
    async def stream_gen() -> AsyncGenerator[bytes]:
        await asyncio.sleep(0)  # Simulate some work
        yield orjson.dumps({"sleep": 0.0})
        if sleep_kind == "server":
            await asyncio.sleep(0)
            yield orjson.dumps({"sleep": 0.1})
        else:
            assert sleep_kind == "stream"
            await asyncio.sleep(0.1)
            yield orjson.dumps({"sleep": 0.0})

    req_builder = client.post(echo_body_parts_server.url).timeout(timedelta(seconds=0.05)).body_stream(stream_gen())
    if read_buffer_limit is not None:
        req_builder = req_builder.streamed_read_buffer_limit(read_buffer_limit)

    req = req_builder.build_streamed()

    if read_buffer_limit is not None:
        assert req.read_buffer_limit == read_buffer_limit
    else:
        assert req.read_buffer_limit == RequestBuilder.default_streamed_read_buffer_limit()

    default_initial_read = RequestBuilder.default_streamed_read_buffer_limit()

    if read_buffer_limit is None or read_buffer_limit >= default_initial_read or sleep_kind == "stream":
        error = ConnectTimeoutError if sleep_kind == "stream" else ReadTimeoutError
        with pytest.raises(error):
            async with req as _:
                pytest.fail("Should have raised")
    else:
        async with req as resp:
            assert (await resp.body_reader.read_chunk()) == orjson.dumps({"sleep": 0.0})
            with pytest.raises(ReadTimeoutError):
                await resp.body_reader.read_chunk()


@pytest.mark.parametrize("read_buffer_limit", [None, 0, 5, 999999])
@pytest.mark.parametrize("partial_body", [False, True])
async def test_body_stream__gen_error(
    client: Client,
    echo_body_parts_server: EchoBodyPartsServer,
    read_buffer_limit: int | None,
    partial_body: bool,
):
    class MyError(Exception): ...

    async def stream_gen() -> AsyncGenerator[bytes]:
        await asyncio.sleep(0)  # Simulate some work
        if partial_body:
            yield b"part 0"
        raise MyError("Test error")

    if read_buffer_limit is not None:
        req = (
            client.post(echo_body_parts_server.url)
            .body_stream(stream_gen())
            .streamed_read_buffer_limit(read_buffer_limit)
            .build_streamed()
        )
    else:
        req = client.post(echo_body_parts_server.url).body_stream(stream_gen()).build_streamed()

    if read_buffer_limit is not None:
        assert req.read_buffer_limit == read_buffer_limit
    else:
        assert req.read_buffer_limit == RequestBuilder.default_streamed_read_buffer_limit()

    with pytest.raises(MyError, match="Test error") as e:
        async with req as _:
            pytest.fail("Should have raised")

    tb_names = [tb.name for tb in traceback.extract_tb(e.value.__traceback__)]
    assert "test_body_stream__gen_error" in tb_names
    assert "stream_gen" in tb_names


async def test_body_stream__invalid_gen(client: Client, echo_body_parts_server: EchoBodyPartsServer):
    async def async_gen() -> AsyncGenerator[int]:
        yield 1

    def gen() -> Generator[int]:
        yield 1

    for case in [async_gen, gen, 1]:
        req = client.post(echo_body_parts_server.url)
        with pytest.raises(TypeError, match="object is not iterable"):
            req.body_stream(case)  # type: ignore[arg-type]


async def test_body_consumed(client: Client, echo_server: EchoServer):
    resp = await client.get(echo_server.url).build().send()

    first = await resp.json()
    assert first["path"] == "/"
    assert (await resp.json()) == first

    first = await resp.text()
    assert '"path":"/"' in first
    assert await resp.text() == first

    first = await resp.bytes()
    assert b'"path":"/"' in first
    assert await resp.bytes() == first

    assert (await resp.body_reader.read_chunk()) is None


async def test_body_consumed__already_started(client: Client, echo_body_parts_server: EchoBodyPartsServer):
    async def stream_gen() -> AsyncGenerator[bytes]:
        yield b"part 0"
        yield b"part 1"

    resp = await client.post(echo_body_parts_server.url).body_stream(stream_gen()).build().send()

    assert await resp.body_reader.read_chunk() == b"part 0"

    with pytest.raises(RuntimeError, match="Response body already consumed"):
        await resp.json()
    with pytest.raises(RuntimeError, match="Response body already consumed"):
        await resp.text()
    with pytest.raises(RuntimeError, match="Response body already consumed"):
        await resp.bytes()

    assert await resp.body_reader.read_chunk() == b"part 1"
    assert not await resp.body_reader.read_chunk()


async def test_body_response_empty(client: Client, echo_body_parts_server: EchoBodyPartsServer):
    async def yield_empty() -> AsyncGenerator[bytes]:
        yield b""

    async def no_yield() -> AsyncGenerator[bytes]:
        if False:
            yield b""

    cases = [yield_empty(), no_yield()]
    for case in cases:
        async with client.post(echo_body_parts_server.url).body_stream(case).build_streamed() as resp:
            assert await resp.body_reader.read_chunk() is None


async def test_use_after_close(client: Client, echo_body_parts_server: EchoBodyPartsServer):
    async def stream_gen() -> AsyncGenerator[bytes]:
        yield b"part 0"
        yield b"part 1"

    req = client.post(echo_body_parts_server.url).body_stream(stream_gen()).build_streamed()

    async with req as resp:
        assert await resp.body_reader.read_chunk() == b"part 0"

    with pytest.raises(RuntimeError, match="Response body reader is closed"):
        _ = resp.body_reader
    with pytest.raises(RuntimeError, match="Response body reader is closed"):
        await resp.json()
    with pytest.raises(RuntimeError, match="Response body reader is closed"):
        await resp.text()
    with pytest.raises(RuntimeError, match="Response body reader is closed"):
        await resp.bytes()
    assert resp.headers["content-type"] == "application/json"
