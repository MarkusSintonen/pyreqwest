from collections.abc import AsyncGenerator, Mapping, Sequence
from datetime import timedelta
from typing import Any

import pytest
import trustme
from pyreqwest.client import Client, ClientBuilder
from pyreqwest.exceptions import BuilderError, ConnectTimeoutError, StatusError
from pyreqwest.http import HeaderMap
from pyreqwest.request import RequestBuilder

from tests.servers.server_subprocess import SubprocessServer
from tests.utils import IS_CI


@pytest.fixture
async def client(cert_authority: trustme.CA) -> AsyncGenerator[Client, None]:
    cert_pem = cert_authority.cert_pem.bytes()
    async with ClientBuilder().error_for_status(True).add_root_certificate_pem(cert_pem).build() as client:
        yield client


async def test_build_consumed(client: Client, echo_body_parts_server: SubprocessServer) -> None:
    sent = "a" * (RequestBuilder.default_streamed_read_buffer_limit() * 3)
    resp = await client.post(echo_body_parts_server.url).body_text(sent).build().send()
    assert (await resp.text()) == sent


async def test_build_streamed(client: Client, echo_body_parts_server: SubprocessServer):
    sent = "a" * (RequestBuilder.default_streamed_read_buffer_limit() * 3)
    async with client.post(echo_body_parts_server.url).body_text(sent).build_streamed() as resp:
        assert (await resp.text()) == sent


@pytest.mark.parametrize("value", [True, False])
async def test_error_for_status(echo_server: SubprocessServer, value: bool):
    url = echo_server.url.with_query({"status": 400})

    async with ClientBuilder().error_for_status(False).build() as client:
        req = client.get(url).error_for_status(value).build()
        if value:
            with pytest.raises(StatusError) as e:
                await req.send()
            assert e.value.details and e.value.details["status"] == 400
        else:
            assert (await req.send()).status == 400


async def test_header(client: Client, echo_server: SubprocessServer):
    resp = await client.get(echo_server.url).header("X-Test", "Val").build().send()
    assert ["x-test", "Val"] in (await resp.json())["headers"]

    with pytest.raises(ValueError, match="invalid HTTP header name"):
        client.get(echo_server.url).header("X-Test\n", "Val\n")

    with pytest.raises(ValueError, match="failed to parse header value"):
        client.get(echo_server.url).header("X-Test", "Val\n")


async def test_headers(client: Client, echo_server: SubprocessServer):
    for type_ in [list, tuple, dict, HeaderMap]:
        headers = type_([("X-Test-1", "Val1"), ("X-Test-2", "Val2")])
        resp = await client.get(echo_server.url).headers(headers).build().send()
        assert ["x-test-1", "Val1"] in (await resp.json())["headers"]
        assert ["x-test-2", "Val2"] in (await resp.json())["headers"]

    headers = HeaderMap([("X-Test", "foo"), ("X-Test", "bar")])
    resp = await client.get(echo_server.url).headers(headers).build().send()
    assert ["x-test", "foo"] in (await resp.json())["headers"]
    assert ["x-test", "bar"] in (await resp.json())["headers"]

    with pytest.raises(ValueError, match="invalid HTTP header name"):
        client.get(echo_server.url).headers({"X-Test\n": "Val\n"})
    with pytest.raises(ValueError, match="failed to parse header value"):
        client.get(echo_server.url).headers({"X-Test": "Val\n"})


@pytest.mark.parametrize("password", ["test_pass", None])
async def test_basic_auth(client: Client, echo_server: SubprocessServer, password: str | None):
    resp = await client.get(echo_server.url).basic_auth("user", password).build().send()
    assert dict((await resp.json())["headers"])["authorization"].startswith("Basic ")


async def test_bearer_auth(client: Client, echo_server: SubprocessServer):
    resp = await client.get(echo_server.url).bearer_auth("test_token").build().send()
    assert dict((await resp.json())["headers"])["authorization"].startswith("Bearer ")


async def test_body_bytes(client: Client, echo_body_parts_server: SubprocessServer):
    body = b"test body"
    resp = await client.post(echo_body_parts_server.url).body_bytes(body).build().send()
    assert (await resp.bytes()) == body


@pytest.mark.parametrize("body", ["test body", "\n\n\n", "🤗🤗🤗"])
async def test_body_text(client: Client, echo_body_parts_server: SubprocessServer, body: str):
    resp = await client.post(echo_body_parts_server.url).body_text(body).build().send()
    assert (await resp.text()) == body


async def test_body_stream(client: Client, echo_body_parts_server: SubprocessServer):
    async def body_stream() -> AsyncGenerator[bytes, None]:
        yield b"part 0"
        yield b"part 1"

    resp = await client.post(echo_body_parts_server.url).body_stream(body_stream()).build().send()
    assert (await resp.body_reader.read_chunk()) == b"part 0"
    assert (await resp.body_reader.read_chunk()) == b"part 1"
    assert (await resp.body_reader.read_chunk()) is None


@pytest.mark.parametrize("server_sleep", [0.1, 0.01, None])
async def test_timeout(client: Client, echo_server: SubprocessServer, server_sleep: float | None):
    timeout = 0.5 if IS_CI else 0.05
    sleep = server_sleep * 10 if IS_CI and server_sleep else server_sleep

    url = echo_server.url.with_query({"sleep_start": sleep or 0})

    req = client.get(url).timeout(timedelta(seconds=timeout)).build()
    if server_sleep and server_sleep > 0.05:
        with pytest.raises(ConnectTimeoutError):
            await req.send()
    else:
        assert await req.send()

    with pytest.raises(TypeError, match="object cannot be converted"):
        client.get(echo_server.url).timeout(1)  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="object cannot be converted"):
        client.get(echo_server.url).timeout(1.0)  # type: ignore[arg-type]


async def test_query(client: Client, echo_server: SubprocessServer):
    async def send(arg: Sequence[tuple[str, str]] | Mapping[str, str]) -> list[list[str]]:
        resp = await client.get(echo_server.url).query(arg).build().send()
        return (await resp.json())["query"]  # type: ignore[no-any-return]

    for arg_type in [list, tuple, dict]:
        assert (await send(arg_type([]))) == []
        assert (await send(arg_type([("foo", "bar")]))) == [["foo", "bar"]]
        assert (await send(arg_type([("foo", "bar"), ("test", "testing")]))) == [["foo", "bar"], ["test", "testing"]]
        assert (await send(arg_type([("foo", 1)]))) == [["foo", "1"]]
        assert (await send(arg_type([("foo", True)]))) == [["foo", "true"]]

    for arg_type in [list, tuple]:
        val = arg_type([("foo", "bar"), ("foo", "baz")])
        resp = await client.get(echo_server.url).query(val).build().send()
        assert (await resp.json())["query"] == [["foo", "bar"], ["foo", "baz"]]


async def test_version(client: Client, echo_server: SubprocessServer, https_echo_server: SubprocessServer):
    resp = await client.get(echo_server.url).build().send()
    assert (await resp.json())["http_version"] == "1.1"
    resp = await client.get(https_echo_server.url).build().send()
    assert (await resp.json())["http_version"] == "2"


async def test_form(client: Client, echo_server: SubprocessServer):
    async def send(arg: Sequence[tuple[str, str]] | Mapping[str, str]) -> str:
        resp = await client.get(echo_server.url).form(arg).build().send()
        return "".join((await resp.json())["body_parts"])

    for arg_type in [list, tuple, dict]:
        assert (await send(arg_type([]))) == ""
        assert (await send(arg_type([("foo", "bar")]))) == "foo=bar"
        assert (await send(arg_type([("foo", "bar"), ("test", "testing")]))) == "foo=bar&test=testing"
        assert (await send(arg_type([("foo", 1)]))) == "foo=1"
        assert (await send(arg_type([("foo", True)]))) == "foo=true"

    for arg_type in [list, tuple]:
        val = arg_type([("foo", "bar"), ("foo", "baz")])
        resp = await client.get(echo_server.url).form(val).build().send()
        assert "".join((await resp.json())["body_parts"]) == "foo=bar&foo=baz"


@pytest.mark.parametrize("case", ["query", "form"])
async def test_form_query_invalid(client: Client, echo_server: SubprocessServer, case: str):
    def build(v: Any) -> RequestBuilder:
        if case == "query":
            return client.get(echo_server.url).query(v)
        assert case == "form"
        return client.get(echo_server.url).form(v)

    with pytest.raises(TypeError, match="object cannot be converted"):
        build("invalid")
    with pytest.raises(TypeError, match="failed to extract"):
        build(None)
    with pytest.raises(TypeError, match="object cannot be converted"):
        build(["a", "b"])
    with pytest.raises(TypeError, match="'int' object cannot be converted to 'PyString'"):
        build([(1, "b")])
    with pytest.raises(BuilderError, match="Failed to build request") as e:
        build([("foo", {"a": "b"})]).build()
    assert e.value.details and {"message": "unsupported value"} in e.value.details["causes"]


async def test_form_fails_with_body_set(client: Client, echo_server: SubprocessServer):
    with pytest.raises(BuilderError, match="Can not set body when multipart or form is used"):
        client.post(echo_server.url).form({"a": "b"}).body_text("fail").build()
    with pytest.raises(BuilderError, match="Can not set body when multipart or form is used"):
        client.post(echo_server.url).body_text("fail").form({"a": "b"}).build()


async def test_extensions(client: Client, echo_server: SubprocessServer):
    myobj = object()
    extensions = {"ext1": "value1", "ext2": "value2", "ext3": myobj}
    resp = await client.get(echo_server.url).extensions(extensions).build().send()
    assert resp.extensions == extensions
    assert resp.extensions["ext3"] == myobj

    resp = await client.get(echo_server.url).extensions({}).build().send()
    assert resp.extensions == {}

    with pytest.raises(TypeError, match="object cannot be converted"):
        client.get(echo_server.url).extensions(1)  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="'int' object cannot be converted to 'PyString'"):
        client.get(echo_server.url).extensions([(1, "b")])  # type: ignore[list-item]
