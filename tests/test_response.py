import asyncio
import gc
import json
import string
import sys
import time
import weakref
from asyncio import Task
from collections.abc import AsyncGenerator, AsyncIterator, Iterator, MutableMapping
from typing import Any

import pytest
import trustme
from pyreqwest.bytes import Bytes
from pyreqwest.client import Client, ClientBuilder
from pyreqwest.exceptions import BodyDecodeError, JSONDecodeError, StatusError
from pyreqwest.http import HeaderMap
from pyreqwest.response import ResponseBuilder

from tests.servers.server_subprocess import SubprocessServer


@pytest.fixture
async def client(cert_authority: trustme.CA) -> AsyncGenerator[Client, None]:
    cert_pem = cert_authority.cert_pem.bytes()
    async with ClientBuilder().error_for_status(True).add_root_certificate_pem(cert_pem).build() as client:
        yield client


async def test_status(client: Client, echo_server: SubprocessServer) -> None:
    req = client.get(echo_server.url).build()
    resp = await req.send()
    resp.error_for_status()
    assert resp.status == 200

    resp.status = 404
    assert resp.status == 404
    with pytest.raises(StatusError, match="HTTP status client error") as e:
        resp.error_for_status()
    assert e.value.details and e.value.details["status"] == 404

    with pytest.raises(ValueError, match="invalid status code"):
        resp.status = 9999


async def test_headers(client: Client, echo_server: SubprocessServer) -> None:
    req = (
        client.get(echo_server.url)
        .query([("header_x_test1", "Value1"), ("header_x_test1", "Value2"), ("header_x_test2", "Value3")])
        .build()
    )
    resp = await req.send()

    assert resp.get_header("x-test1") == "Value1" and resp.get_header("x-test2") == "Value3"
    assert resp.get_header_all("x-test1") == ["Value1", "Value2"] and resp.get_header_all("x-test2") == ["Value3"]
    assert resp.headers.getall("X-Test1") == ["Value1", "Value2"] and resp.headers["x-test1"] == "Value1"
    assert resp.headers.getall("X-Test2") == ["Value3"] and resp.headers["x-test2"] == "Value3"

    resp.headers["X-Test2"] = "Value4"
    assert resp.headers["X-Test2"] == "Value4" and resp.headers["x-test2"] == "Value4"
    assert resp.get_header("x-test2") == "Value4" and resp.get_header_all("x-test2") == ["Value4"]

    assert resp.headers.popall("x-test1") == ["Value1", "Value2"]
    assert "X-Test1" not in resp.headers and "x-test1" not in resp.headers
    assert resp.get_header("x-test1") is None
    assert resp.get_header("Content-Type") == "application/json"

    resp.headers = {"X-New": "NewValue"}
    assert list(resp.headers.items()) == [("x-new", "NewValue")]
    assert resp.get_header("Content-Type") is None

    assert type(resp.headers) is HeaderMap and isinstance(resp.headers, MutableMapping)


@pytest.mark.parametrize("proto", ["http", "https"])
async def test_version(
    client: Client, echo_server: SubprocessServer, https_echo_server: SubprocessServer, proto: str
) -> None:
    url = echo_server.url if proto == "http" else https_echo_server.url
    resp = await client.get(url).build().send()
    if proto == "http":
        assert resp.version == "HTTP/1.1"
    else:
        assert proto == "https"
        assert resp.version == "HTTP/2.0"

    for v in ["HTTP/0.9", "HTTP/1.0", "HTTP/1.1", "HTTP/2.0", "HTTP/3.0"]:
        resp.version = v
        assert resp.version == v

    with pytest.raises(ValueError, match="invalid http version"):
        resp.version = "foobar"


async def test_extensions(client: Client, echo_server: SubprocessServer) -> None:
    req = client.get(echo_server.url).extensions({"a": "b"}).build()
    req.extensions["c"] = "d"
    resp = await req.send()
    assert resp.extensions == {"a": "b", "c": "d"}
    resp.extensions["c"] = "e"
    assert resp.extensions == {"a": "b", "c": "e"}
    resp.extensions = {"foo": "bar", "test": "value"}
    assert resp.extensions.pop("test") == "value"
    assert resp.extensions == {"foo": "bar"}
    resp.extensions = [("x", 1), ("y", 2)]
    assert resp.extensions == {"x": 1, "y": 2}


@pytest.mark.parametrize("kind", ["chunk", "bytes", "text", "json"])
async def test_body(client: Client, echo_body_parts_server: SubprocessServer, kind: str) -> None:
    async def stream_gen() -> AsyncGenerator[bytes, None]:
        yield b'{"foo": "bar", "test": "value"'
        yield b', "baz": 123}'

    resp = await client.post(echo_body_parts_server.url).body_stream(stream_gen()).build().send()
    if kind == "chunk":
        assert (await resp.body_reader.read_chunk()) == b'{"foo": "bar", "test": "value"'
        assert (await resp.body_reader.read_chunk()) == b', "baz": 123}'
        assert (await resp.body_reader.read_chunk()) is None
        with pytest.raises(RuntimeError, match="Response body already consumed"):
            await resp.bytes()
        with pytest.raises(RuntimeError, match="Response body already consumed"):
            await resp.text()
        with pytest.raises(RuntimeError, match="Response body already consumed"):
            await resp.json()
    elif kind == "bytes":
        assert (await resp.bytes()) == b'{"foo": "bar", "test": "value", "baz": 123}'
        assert (await resp.bytes()) == b'{"foo": "bar", "test": "value", "baz": 123}'
        assert (await resp.body_reader.read_chunk()) is None
    elif kind == "text":
        assert (await resp.text()) == '{"foo": "bar", "test": "value", "baz": 123}'
        assert (await resp.text()) == '{"foo": "bar", "test": "value", "baz": 123}'
        assert (await resp.body_reader.read_chunk()) is None
    else:
        assert kind == "json"
        assert (await resp.json()) == {"foo": "bar", "test": "value", "baz": 123}
        assert (await resp.json()) == {"foo": "bar", "test": "value", "baz": 123}
        assert (await resp.body_reader.read_chunk()) is None


async def test_body_reader_read(client: Client, echo_body_parts_server: SubprocessServer) -> None:
    chars = string.ascii_letters + string.digits
    body = b"".join(chars[v % len(chars)].encode() for v in range(131072))

    async def stream_gen() -> AsyncGenerator[bytes, None]:
        yield body

    resp = await client.post(echo_body_parts_server.url).body_stream(stream_gen()).build().send()
    assert (await resp.body_reader.read()) == body[:65536]
    assert (await resp.body_reader.read()) == body[65536:]
    assert (await resp.body_reader.read()) is None

    resp = await client.post(echo_body_parts_server.url).body_stream(stream_gen()).build().send()
    assert (await resp.body_reader.read(0)) == b""
    assert (await resp.body_reader.read(100)) == body[:100]
    assert (await resp.body_reader.read(100)) == body[100:200]
    assert (await resp.body_reader.read(131072)) == body[200:]
    assert (await resp.body_reader.read(10)) is None


ASCII_TEST = b"""
{
  "a": "qwe",
  "b": "qweqwe",
  "c": "qweq",
  "d: "qwe"
}
"""
MULTILINE_EMOJI = """[
    "😊",
    "a"
"""


@pytest.mark.parametrize(
    "body",
    [
        pytest.param("", id="empty"),
        pytest.param(ASCII_TEST, id="ascii"),
        pytest.param('["üýþÿ", "a" ', id="latin1"),
        pytest.param('["東京", "a" ', id="two-byte"),
        pytest.param(b'["\xe6\x9d\xb1\xe4\xba\xac", "a" ', id="two-byte-bytes"),
        pytest.param(MULTILINE_EMOJI, id="four-byte-multiline"),
        pytest.param('["tab	character	in	string	"]', id="tabs"),
    ],
)
async def test_bad_json(client: Client, echo_body_parts_server: SubprocessServer, body: str | bytes) -> None:
    body_bytes = body if isinstance(body, bytes) else body.encode("utf8")
    body_str = body_bytes.decode("utf8")

    async def stream_gen() -> AsyncGenerator[bytes, None]:
        yield body_bytes

    resp = await client.post(echo_body_parts_server.url).body_stream(stream_gen()).build().send()
    with pytest.raises(JSONDecodeError) as e:
        await resp.json()
    assert isinstance(e.value, json.JSONDecodeError)
    assert isinstance(e.value, BodyDecodeError)

    with pytest.raises(json.JSONDecodeError) as std_err:  # Compare error against standard json decoder
        json.loads(body)

    last_line = body_str.split("\n")[e.value.lineno - 1]
    # Position is given as byte based to avoid calculating position based on UTF8 chars
    assert body_bytes[: e.value.pos].decode("utf8") == body_str[: std_err.value.pos]
    assert last_line.encode("utf8")[: e.value.colno - 1].decode("utf8") == last_line[: std_err.value.colno - 1]
    assert e.value.lineno == std_err.value.lineno

    assert e.value.details == {"causes": None}


@pytest.mark.parametrize(
    ("body", "charset", "expect"),
    [
        pytest.param(b"ascii text", "ascii", "ascii text", id="ascii"),
        pytest.param("ascii bäd".encode(), "ascii", "ascii bÃ¤d", id="ascii_bad"),
        pytest.param("utf-8 text 😊".encode(), "utf-8", "utf-8 text 😊", id="utf8"),
        pytest.param(b"utf-8 bad \xe2\x82", "utf-8", "utf-8 bad �", id="utf8_bad"),
        pytest.param("utf-8 text 😊".encode(), None, "utf-8 text 😊", id="utf8_default"),
    ],
)
async def test_text(
    client: Client,
    echo_body_parts_server: SubprocessServer,
    body: bytes,
    charset: str | None,
    expect: str,
) -> None:
    async def resp_body() -> AsyncGenerator[bytes]:
        yield body

    content_type = f"text/plain; charset={charset}" if charset else "text/plain"
    resp = (
        await client.post(echo_body_parts_server.url)
        .body_stream(resp_body())
        .query({"content_type": content_type})
        .build()
        .send()
    )
    mime = resp.content_type_mime()
    assert mime and mime.get_param("charset") == charset
    assert await resp.text() == expect


async def test_mime(client: Client, echo_body_parts_server: SubprocessServer) -> None:
    async def resp_body() -> AsyncGenerator[bytes]:
        yield b"test"

    resp = (
        await client.post(echo_body_parts_server.url)
        .body_stream(resp_body())
        .query(
            {"content_type": "text/plain;charset=ascii"},
        )
        .build()
        .send()
    )

    mime = resp.content_type_mime()
    assert mime and mime.type_ == "text" and mime.subtype == "plain" and mime.get_param("charset") == "ascii"
    assert str(mime) == "text/plain;charset=ascii" and repr(mime) == "Mime('text/plain;charset=ascii')"

    resp.headers["content-type"] = "application/json;charset=utf8"
    mime = resp.content_type_mime()
    assert mime and mime.type_ == "application" and mime.subtype == "json" and mime.get_param("charset") == "utf8"

    assert resp.headers.pop("content-type") == "application/json;charset=utf8"
    assert resp.content_type_mime() is None


async def test_error_for_status(echo_server: SubprocessServer) -> None:
    async with ClientBuilder().build() as client:
        resp = await client.get(echo_server.url).query([("status", 201)]).build().send()
        resp.error_for_status()

        resp = await client.get(echo_server.url).query([("status", 404)]).build().send()
        with pytest.raises(StatusError, match="HTTP status client error") as e:
            resp.error_for_status()
        assert e.value.details and e.value.details["status"] == 404

        resp = await client.get(echo_server.url).query([("status", 500)]).build().send()
        with pytest.raises(StatusError, match="HTTP status server error") as e:
            resp.error_for_status()
        assert e.value.details and e.value.details["status"] == 500


@pytest.mark.parametrize("read", ["bytes", "text", "json", "reader_bytes", "read", "read_chunk"])
async def test_response_read_cancel(client: Client, echo_body_parts_server: SubprocessServer, read: str) -> None:
    buf_size = 1024
    full_buf = "a" * (buf_size * 2)
    max_cancel_delay = 1.0

    async def stream_gen() -> AsyncGenerator[bytes, None]:
        for _ in range(10):
            await asyncio.sleep(0)
            yield json.dumps({"sleep": 0.2, "data": full_buf}).encode()

    req = (
        client.post(echo_body_parts_server.url)
        .body_stream(stream_gen())
        .streamed_read_buffer_limit(buf_size)
        .build_streamed()
    )

    async with req as resp:
        if read == "bytes":
            task: Task[Any] = asyncio.create_task(resp.bytes())
        elif read == "text":
            task = asyncio.create_task(resp.text())
        elif read == "json":
            task = asyncio.create_task(resp.json())
        elif read == "reader_bytes":
            task = asyncio.create_task(resp.body_reader.bytes())
        elif read == "read":

            async def reader() -> None:
                while await resp.body_reader.read(buf_size) is not None:
                    pass
                raise RuntimeError("Should not reach here")

            task = asyncio.create_task(reader())
        else:
            assert read == "read_chunk"

            async def reader() -> None:
                while await resp.body_reader.read_chunk() is not None:
                    pass
                raise RuntimeError("Should not reach here")

            task = asyncio.create_task(reader())

        start = time.time()
        await asyncio.sleep(0.1)  # Allow the request to start processing
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task
        assert time.time() - start < max_cancel_delay


async def test_response_builder():
    async def stream() -> AsyncIterator[bytes]:
        yield b"test1 "
        yield b"test2"

    resp = (
        await ResponseBuilder()
        .status(201)
        .headers([("X-Test", "Value1"), ("X-Test", "Value2")])
        .header("X-Test", "Value3")
        .header("X-Test2", "Value4")
        .extensions({"foo": "bar"})
        .version("HTTP/2.0")
        .body_stream(stream())
        .build()
    )

    assert resp.headers["X-Test"] == "Value1"
    assert resp.headers.getall("X-Test") == ["Value1", "Value2", "Value3"]
    assert resp.headers.getall("X-Test2") == ["Value4"]
    assert resp.extensions == {"foo": "bar"}
    assert resp.status == 201
    assert await resp.bytes() == b"test1 test2"
    assert resp.version == "HTTP/2.0"


def test_response_builder__sync():
    def stream() -> Iterator[bytes]:
        yield b"test1 "
        yield b"test2"

    resp = (
        ResponseBuilder()
        .status(201)
        .headers([("X-Test", "Value1"), ("X-Test", "Value2")])
        .header("X-Test", "Value3")
        .header("X-Test2", "Value4")
        .extensions({"foo": "bar"})
        .version("HTTP/2.0")
        .body_stream(stream())
        .build_sync()
    )

    assert resp.headers["X-Test"] == "Value1"
    assert resp.headers.getall("X-Test") == ["Value1", "Value2", "Value3"]
    assert resp.headers.getall("X-Test2") == ["Value4"]
    assert resp.extensions == {"foo": "bar"}
    assert resp.status == 201
    assert resp.bytes() == b"test1 test2"
    assert resp.version == "HTTP/2.0"


async def test_response_builder__sync_no_async_mix() -> None:
    async def stream() -> AsyncIterator[bytes]:
        pytest.fail("Should not be called")
        yield b""

    builder = ResponseBuilder().body_stream(stream())
    with pytest.raises(ValueError, match="Cannot use async iterator in a blocking context"):
        builder.build_sync()


async def test_bytes_buffer_abc(client: Client, echo_body_parts_server: SubprocessServer) -> None:
    resp = await client.get(echo_body_parts_server.url).build().send()
    buf = await resp.bytes()
    assert type(buf) is Bytes
    if sys.version_info >= (3, 12):
        from collections.abc import Buffer

        assert isinstance(buf, Buffer) and issubclass(type(buf), Buffer)


def test_response_builder__circular_reference_collected() -> None:
    # Check the GC support via __traverse__ and __clear__
    ref: weakref.ReferenceType[Any] | None = None

    def check() -> None:
        nonlocal ref

        class StreamHandler:
            def __init__(self) -> None:
                self.builder: ResponseBuilder | None = None

            def __aiter__(self) -> AsyncGenerator[bytes]:
                async def gen() -> AsyncGenerator[bytes]:
                    yield b"test"

                return gen()

        stream = StreamHandler()
        resp = ResponseBuilder().body_stream(stream)
        stream.builder = resp
        ref = weakref.ref(stream)

    check()
    gc.collect()
    assert ref is not None and ref() is None
