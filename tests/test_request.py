from typing import AsyncGenerator

import pytest
import trustme

from pyreqwest.client import Client, ClientBuilder
from pyreqwest.exceptions import RequestError
from pyreqwest.http import Body
from pyreqwest.http.types import Stream
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
    assert (await resp.json())['method'] == 'POST'


async def test_url(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).query({"a": "b"}).build_consumed()
    assert req.url == echo_server.url.with_query({"a": "b"})
    req.url = req.url.with_query({"test": "value"})
    assert req.url.query == {"test": "value"}


async def test_headers(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).headers({"X-Test1": "Value1", "X-Test2": "Value2"}).build_consumed()
    assert req.headers["X-Test1"] == "Value1" and req.headers["x-test1"] == "Value1"
    assert req.headers["X-Test2"] == "Value2" and req.headers["x-test2"] == "Value2"

    req.headers["X-Test3"] = "Value3"
    assert req.headers["X-Test3"] == "Value3" and req.headers["x-test3"] == "Value3"

    assert req.headers.pop("x-test1")
    assert "X-Test1" not in req.headers and "x-test1" not in req.headers

    resp = await req.send()
    assert [(k, v) for k, v in (await resp.json())['headers'] if k.startswith("x-")] == [
        ('x-test2', 'Value2'), ('x-test3', 'Value3')
    ]


@pytest.mark.parametrize("kind", ["bytes", "text"])
async def test_body__content(client: Client, echo_server: Server, kind: str) -> None:
    def body() -> Body:
        if kind == "bytes":
            return Body.from_bytes(b"test1")
        else:
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
async def test_body__stream(
    client: Client, echo_server: Server, yield_type: type[bytes] | type[bytearray] | type[memoryview]
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


async def test_body__stream_error_already_used(client: Client, echo_server: Server) -> None:
    async def stream_gen() -> Stream:
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
