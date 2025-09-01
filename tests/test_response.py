import json
import string
from collections.abc import AsyncGenerator, MutableMapping

import pytest
import trustme
from pyreqwest.client import Client, ClientBuilder
from pyreqwest.exceptions import DecodeError, JSONDecodeError, StatusError
from pyreqwest.http import HeaderMap

from .servers.server import Server


@pytest.fixture
async def client(cert_authority: trustme.CA) -> AsyncGenerator[Client, None]:
    cert_pem = cert_authority.cert_pem.bytes()
    async with ClientBuilder().error_for_status(True).add_root_certificate_pem(cert_pem).build() as client:
        yield client


async def test_status(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).build_consumed()
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


async def test_headers(client: Client, echo_server: Server) -> None:
    req = (
        client.get(echo_server.url)
        .query(
            [("header_x_test1", "Value1"), ("header_x_test1", "Value2"), ("header_x_test2", "Value3")],
        )
        .build_consumed()
    )
    resp = await req.send()

    assert type(resp.headers) is HeaderMap and isinstance(resp.headers, MutableMapping)

    assert resp.headers.getall("X-Test1") == ["Value1", "Value2"] and resp.headers["x-test1"] == "Value1"
    assert resp.headers.getall("X-Test2") == ["Value3"] and resp.headers["x-test2"] == "Value3"

    resp.headers["X-Test2"] = "Value4"
    assert resp.headers["X-Test2"] == "Value4" and resp.headers["x-test2"] == "Value4"

    assert resp.headers.popall("x-test1") == ["Value1", "Value2"]
    assert "X-Test1" not in resp.headers and "x-test1" not in resp.headers


@pytest.mark.parametrize("proto", ["http", "https"])
async def test_version(client: Client, echo_server: Server, https_echo_server: Server, proto: str) -> None:
    url = echo_server.url if proto == "http" else https_echo_server.url
    resp = await client.get(url).build_consumed().send()
    if proto == "http":
        assert resp.version == "HTTP/1.1"
    else:
        assert proto == "https"
        assert resp.version == "HTTP/2.0"

    resp.version = "HTTP/3.0"
    assert resp.version == "HTTP/3.0"

    with pytest.raises(ValueError, match="invalid http version"):
        resp.version = "foobar"


async def test_extensions(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).extensions({"a": "b"}).build_consumed()
    req.extensions["c"] = "d"
    resp = await req.send()
    assert resp.extensions == {"a": "b", "c": "d"}
    resp.extensions["c"] = "e"
    assert resp.extensions == {"a": "b", "c": "e"}
    resp.extensions = {"foo": "bar", "test": "value"}
    assert resp.extensions.pop("test") == "value"
    assert resp.extensions == {"foo": "bar"}


@pytest.mark.parametrize("kind", ["chunk", "bytes", "text", "json"])
async def test_body(client: Client, echo_body_parts_server: Server, kind: str) -> None:
    async def stream_gen() -> AsyncGenerator[bytes, None]:
        yield b'{"foo": "bar", "test": "value"'
        yield b', "baz": 123}'

    resp = await client.post(echo_body_parts_server.url).body_stream(stream_gen()).build_consumed().send()
    if kind == "chunk":
        assert (await resp.next_chunk()) == b'{"foo": "bar", "test": "value"'
        assert (await resp.next_chunk()) == b', "baz": 123}'
        assert (await resp.next_chunk()) is None
        with pytest.raises(RuntimeError, match="Response body already consumed"):
            await resp.bytes()
        with pytest.raises(RuntimeError, match="Response body already consumed"):
            await resp.text()
        with pytest.raises(RuntimeError, match="Response body already consumed"):
            await resp.json()
    elif kind == "bytes":
        assert (await resp.bytes()) == b'{"foo": "bar", "test": "value", "baz": 123}'
        assert (await resp.bytes()) == b'{"foo": "bar", "test": "value", "baz": 123}'
        assert (await resp.next_chunk()) is None
    elif kind == "text":
        assert (await resp.text()) == '{"foo": "bar", "test": "value", "baz": 123}'
        assert (await resp.text()) == '{"foo": "bar", "test": "value", "baz": 123}'
        assert (await resp.next_chunk()) is None
    else:
        assert kind == "json"
        assert (await resp.json()) == {"foo": "bar", "test": "value", "baz": 123}
        assert (await resp.json()) == {"foo": "bar", "test": "value", "baz": 123}
        assert (await resp.next_chunk()) is None


async def test_read(client: Client, echo_body_parts_server: Server) -> None:
    chars = string.ascii_letters + string.digits
    body = b"".join(chars[v % len(chars)].encode() for v in range(131072))

    async def stream_gen() -> AsyncGenerator[bytes, None]:
        yield body

    resp = await client.post(echo_body_parts_server.url).body_stream(stream_gen()).build_consumed().send()
    assert (await resp.read()) == body[:65536]
    assert (await resp.read()) == body[65536:]
    assert (await resp.read()) == b""

    resp = await client.post(echo_body_parts_server.url).body_stream(stream_gen()).build_consumed().send()
    assert (await resp.read(0)) == b""
    assert (await resp.read(100)) == body[:100]
    assert (await resp.read(100)) == body[100:200]
    assert (await resp.read(131072)) == body[200:]
    assert (await resp.read(10)) == b""


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
async def test_bad_json(client: Client, echo_body_parts_server: Server, body: str | bytes) -> None:
    body_bytes = body if isinstance(body, bytes) else body.encode("utf8")
    body_str = body_bytes.decode("utf8")

    async def stream_gen() -> AsyncGenerator[bytes, None]:
        yield body_bytes

    resp = await client.post(echo_body_parts_server.url).body_stream(stream_gen()).build_consumed().send()
    with pytest.raises(JSONDecodeError) as e:
        await resp.json()
    assert isinstance(e.value, json.JSONDecodeError)
    assert isinstance(e.value, DecodeError)

    with pytest.raises(json.JSONDecodeError) as std_err:
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
    echo_body_parts_server: Server,
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
        .build_consumed()
        .send()
    )
    mime = resp.content_type_mime()
    assert mime and mime.get_param("charset") == charset
    assert await resp.text() == expect


async def test_mime(client: Client, echo_body_parts_server: Server) -> None:
    async def resp_body() -> AsyncGenerator[bytes]:
        yield b"test"

    resp = (
        await client.post(echo_body_parts_server.url)
        .body_stream(resp_body())
        .query(
            {"content_type": "text/plain;charset=ascii"},
        )
        .build_consumed()
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


async def test_error_for_status(echo_server: Server) -> None:
    async with ClientBuilder().build() as client:
        resp = await client.get(echo_server.url).query([("status", 201)]).build_consumed().send()
        resp.error_for_status()

        resp = await client.get(echo_server.url).query([("status", 404)]).build_consumed().send()
        with pytest.raises(StatusError, match="HTTP status client error") as e:
            resp.error_for_status()
        assert e.value.details and e.value.details["status"] == 404
