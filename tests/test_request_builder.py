import base64
from collections.abc import AsyncGenerator
from datetime import timedelta

import pytest
import trustme
from pyreqwest.client import Client, ClientBuilder
from pyreqwest.exceptions import BuilderError, ConnectTimeoutError, StatusError
from pyreqwest.http import HeaderMap
from pyreqwest.request import RequestBuilder
from pyreqwest.types import FormParams, QueryParams

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


@pytest.mark.parametrize("value", [True, False, None])
@pytest.mark.parametrize("kwarg", [True, False])
async def test_error_for_status(echo_server: SubprocessServer, value: bool | None, kwarg: bool):
    url = echo_server.url.with_query({"status": 400})

    async with ClientBuilder().error_for_status(False).build() as client:
        if value is None:
            req_builder = client.get(url).error_for_status()
        else:
            req_builder = (
                client.get(url).error_for_status(enable=value) if kwarg else client.get(url).error_for_status(value)
            )

        req = req_builder.build()
        if value or value is None:
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


async def test_header__appends(client: Client, echo_server: SubprocessServer):
    resp = await client.get(echo_server.url).header("X-Test", "Foo").header("X-Test", "Bar").build().send()
    got = (await resp.json())["headers"]
    assert [(k, v) for k, v in got if "x-test" in k] == [("x-test", "Foo"), ("x-test", "Bar")]


async def test_header__json_correct_content_type(client: Client, echo_server: SubprocessServer):
    resp = (
        await client.post(echo_server.url).header("content-type", "application/json").body_json({"a": 1}).build().send()
    )
    got = (await resp.json())["headers"]
    assert [(k, v) for k, v in got if k == "content-type"] == [("content-type", "application/json")]

    resp = await client.post(echo_server.url).header("content-type", "text/plain").body_json({"a": 1}).build().send()
    got = (await resp.json())["headers"]
    assert [(k, v) for k, v in got if k == "content-type"] == [("content-type", "application/json")]


async def test_header__sensitive(client: Client, echo_server: SubprocessServer):
    req = client.get(echo_server.url).header("X-Test", "Val", is_sensitive=True).build()
    assert repr(req.headers) == "HeaderMap({'x-test': 'Sensitive'})"

    req = client.get(echo_server.url).bearer_auth("test").build()
    assert repr(req.headers) == "HeaderMap({'authorization': 'Sensitive'})"

    req = client.get(echo_server.url).basic_auth("user", "pass").build()
    assert repr(req.headers) == "HeaderMap({'authorization': 'Sensitive'})"

    headers = HeaderMap()
    headers.append("X-Test", "Val", is_sensitive=True)
    req = client.get(echo_server.url).headers(headers).build()
    assert repr(req.headers) == "HeaderMap({'x-test': 'Sensitive'})"


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

    with pytest.raises(ValueError, match="Invalid header key: X-Test\n"):
        client.get(echo_server.url).headers({"X-Test\n": "Val\n"})
    with pytest.raises(ValueError, match="Invalid header value: Val\n"):
        client.get(echo_server.url).headers({"X-Test": "Val\n"})


async def test_headers__replaces(client: Client, echo_server: SubprocessServer):
    headers1 = HeaderMap([("X-Test", "foo")])
    headers2 = HeaderMap([("X-Test", "bar"), ("X-Test2", "baz")])
    resp = await client.get(echo_server.url).headers(headers1).headers(headers2).build().send()
    got = (await resp.json())["headers"]
    assert [(k, v) for k, v in got if "x-test" in k] == [("x-test", "bar"), ("x-test2", "baz")]


@pytest.mark.parametrize("password", ["test_pass", None])
async def test_basic_auth(client: Client, echo_server: SubprocessServer, password: str | None):
    resp = await client.get(echo_server.url).basic_auth("user", password).build().send()
    auth = str(dict((await resp.json())["headers"])["authorization"])
    assert auth.startswith("Basic ")
    assert base64.standard_b64decode(auth.removeprefix("Basic ")).decode() == f"user:{password or ''}"


async def test_bearer_auth(client: Client, echo_server: SubprocessServer):
    resp = await client.get(echo_server.url).bearer_auth("test_token").build().send()
    headers = (await resp.json())["headers"]
    assert [(k, v) for k, v in headers if k == "authorization"] == [("authorization", "Bearer test_token")], headers


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

    with pytest.raises(TypeError, match="'int' object is not an instance of 'timedelta'"):
        client.get(echo_server.url).timeout(1)  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="'float' object is not an instance of 'timedelta'"):
        client.get(echo_server.url).timeout(1.0)  # type: ignore[arg-type]


async def test_query(client: Client, echo_server: SubprocessServer):
    async def send(arg: QueryParams) -> list[list[str]]:
        resp = await client.get(echo_server.url).query(arg).build().send()
        return (await resp.json())["query"]  # type: ignore[no-any-return]

    for arg_type in [list, tuple, dict]:
        assert (await send(arg_type([]))) == []
        assert (await send(arg_type([("foo", "bar")]))) == [["foo", "bar"]]
        assert (await send(arg_type([("foo", "bar"), ("test", "testing")]))) == [["foo", "bar"], ["test", "testing"]]
        assert (await send(arg_type([("foo", 1)]))) == [["foo", "1"]]
        assert (await send(arg_type([("foo", 1.0)]))) == [["foo", "1.0"]]
        assert (await send(arg_type([("foo", True)]))) == [["foo", "true"]]
        assert (await send(arg_type([("foo", True), ("bar", 1)]))) == [["foo", "true"], ["bar", "1"]]

    for arg_type in [list, tuple]:
        val = arg_type([("foo", "bar"), ("foo", "baz")])
        resp = await client.get(echo_server.url).query(val).build().send()
        assert (await resp.json())["query"] == [["foo", "bar"], ["foo", "baz"]]

    assert (await send({"foo": ["bar", 1], "test": "x"})) == [["foo", "bar"], ["foo", "1"], ["test", "x"]]
    assert (await send([("foo", ["bar", "baz"])])) == [["foo", "bar"], ["foo", "baz"]]


async def test_form(client: Client, echo_server: SubprocessServer):
    async def send(arg: FormParams) -> str:
        resp = await client.get(echo_server.url).form(arg).build().send()
        return "".join((await resp.json())["body_parts"])

    for arg_type in [list, tuple, dict]:
        assert (await send(arg_type([]))) == ""
        assert (await send(arg_type([("foo", "bar")]))) == "foo=bar"
        assert (await send(arg_type([("foo", "bar"), ("test", "testing")]))) == "foo=bar&test=testing"
        assert (await send(arg_type([("foo", 1)]))) == "foo=1"
        assert (await send(arg_type([("foo", 1.0)]))) == "foo=1.0"
        assert (await send(arg_type([("foo", True)]))) == "foo=true"
        assert (await send(arg_type([("foo", True), ("bar", 1)]))) == "foo=true&bar=1"

    for arg_type in [list, tuple]:
        val = arg_type([("foo", "bar"), ("foo", "baz")])
        resp = await client.get(echo_server.url).form(val).build().send()
        assert "".join((await resp.json())["body_parts"]) == "foo=bar&foo=baz"

    assert (await send({"foo": ["bar", 1], "test": "x"})) == "foo=bar&foo=1&test=x"
    assert (await send([("foo", ["bar", "baz"])])) == "foo=bar&foo=baz"


@pytest.mark.parametrize("case", ["query", "form"])
async def test_query_form_invalid(client: Client, echo_server: SubprocessServer, case: str):
    def build(v: QueryParams | FormParams) -> RequestBuilder:
        if case == "query":
            return client.get(echo_server.url).query(v)
        assert case == "form"
        return client.get(echo_server.url).form(v)

    with pytest.raises(TypeError, match="'str' object is not an instance of 'tuple'"):
        build("invalid")  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="failed to extract"):
        build(None)  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="'str' object is not an instance of 'tuple'"):
        build(["a", "b"])  # type: ignore[list-item]

    for type_ in [list, dict]:
        with pytest.raises(ValueError, match=f"Invalid {case} key: 1"):
            build(type_([(1, "b")]))
        with pytest.raises(ValueError, match=f"Invalid {case} value: {{'a': 'b'}}"):
            build(type_([("foo", {"a": "b"})])).build()
        with pytest.raises(ValueError, match=f"Invalid {case} value: None"):
            build(type_([("foo", None)])).build()


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

    with pytest.raises(TypeError, match="failed to extract"):
        client.get(echo_server.url).extensions(1)  # type: ignore[arg-type]
    with pytest.raises(ValueError, match="Invalid extensions key: 1"):
        client.get(echo_server.url).extensions([(1, "b")])  # type: ignore[list-item]
