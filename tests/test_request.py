import asyncio
import traceback
from datetime import timedelta
from typing import AsyncGenerator

import pytest
from orjson import orjson

from pyreqwest.client import ClientBuilder, Client
from pyreqwest.exceptions import StatusError, ReadTimeoutError, ConnectTimeoutError
from pyreqwest.http import Url
from .servers.echo_body_parts_server import EchoBodyPartsServer
from .servers.echo_server import EchoServer


@pytest.mark.parametrize("value", [True, False])
async def test_error_for_status(echo_server: EchoServer, value: bool):
    url = Url(str(echo_server.address))
    url.set_query_dict({"status": str(400)})

    async with ClientBuilder().error_for_status(False).build() as client:
        req = client.get(url).error_for_status(value).build_consumed()
        if value:
            with pytest.raises(StatusError) as e:
                await req.send()
            assert e.value.details["status"] == 400
        else:
            assert (await req.send()).status == 400


@pytest.fixture
async def client() -> AsyncGenerator[Client, None]:
    async with ClientBuilder().error_for_status(True).build() as client:
        yield client


@pytest.mark.parametrize("initial_read_size", [None, 0, 10, 999999])
@pytest.mark.parametrize("yield_type", [bytes, bytearray, memoryview])
async def test_body_stream(
    client: Client, echo_body_parts_server: EchoBodyPartsServer, initial_read_size: int | None, yield_type: type
):
    async def stream_gen():
        for i in range(5):
            await asyncio.sleep(0)  # Simulate some work
            yield yield_type(f"part {i}".encode("utf-8"))

    req = client.post(echo_body_parts_server.address).body_stream(stream_gen()).build_streamed()
    if initial_read_size is not None:
        req.initial_read_size = initial_read_size
        assert req.initial_read_size == initial_read_size
    else:
        assert req.initial_read_size == 65536

    async with req as resp:
        chunks = []
        while (chunk := await resp.next_chunk()) is not None:
            chunks.append(chunk)
        assert chunks == [b"part 0", b"part 1", b"part 2", b"part 3", b"part 4"]


@pytest.mark.parametrize("initial_read_size", [None, 0, 5, 999999])
@pytest.mark.parametrize("sleep_kind", ["server", "stream"])
async def test_body_stream__timeout(
    client: Client, echo_body_parts_server: EchoBodyPartsServer, initial_read_size: int | None, sleep_kind: str
):
    async def stream_gen():
        await asyncio.sleep(0)  # Simulate some work
        yield orjson.dumps({"sleep": 0.0})
        if sleep_kind == "server":
            await asyncio.sleep(0)
            yield orjson.dumps({"sleep": 0.1})
        else:
            assert sleep_kind == "stream"
            await asyncio.sleep(0.1)
            yield orjson.dumps({"sleep": 0.0})

    req = client.post(echo_body_parts_server.address).timeout(timedelta(seconds=0.05)).body_stream(stream_gen()).build_streamed()
    if initial_read_size is not None:
        req.initial_read_size = initial_read_size

    if initial_read_size is None or initial_read_size >= 65536 or sleep_kind == "stream":
        error = ConnectTimeoutError if sleep_kind == "stream" else ReadTimeoutError
        with pytest.raises(error):
            async with req as _:
                assert False
    else:
        async with req as resp:
            assert (await resp.next_chunk()) == orjson.dumps({"sleep": 0.0})
            with pytest.raises(ReadTimeoutError):
                await resp.next_chunk()


@pytest.mark.parametrize("initial_read_size", [None, 0, 5, 999999])
@pytest.mark.parametrize("partial_body", [False, True])
async def test_body_stream__gen_error(
    client: Client, echo_body_parts_server: EchoBodyPartsServer, initial_read_size: int | None, partial_body: bool
):
    class MyError(Exception): ...

    async def stream_gen():
        await asyncio.sleep(0)  # Simulate some work
        if partial_body:
            yield b"part 0"
        raise MyError("Test error")

    req = client.post(echo_body_parts_server.address).body_stream(stream_gen()).build_streamed()
    if initial_read_size is not None:
        req.initial_read_size = initial_read_size

    with pytest.raises(MyError, match="Test error") as e:
        async with req as _:
            assert False

    tb_names = [tb.name for tb in traceback.extract_tb(e.value.__traceback__)]
    assert "test_stream__gen_error" in tb_names
    assert "stream_gen" in tb_names


async def test_body_stream__invalid_gen(client: Client, echo_body_parts_server: EchoBodyPartsServer):
    def gen():
        yield 1

    async def async_gen():
        yield 1

    cases = [gen(), gen, async_gen, b"123", [b"123"]]

    for case in cases:
        req = client.post(echo_body_parts_server.address).body_stream(case).build_streamed()

        with pytest.raises(TypeError, match="object is not an async iterator"):
            async with req as _:
                assert False
