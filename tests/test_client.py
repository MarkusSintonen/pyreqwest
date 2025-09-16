import asyncio
import json
from collections.abc import Mapping
from datetime import timedelta
from typing import Any

import pytest
import trustme
from cryptography import x509
from cryptography.hazmat.primitives import serialization
from pyreqwest.client import BaseClient, BaseClientBuilder, Client, ClientBuilder, Runtime
from pyreqwest.client.types import JsonDumpsContext, JsonLoadsContext
from pyreqwest.exceptions import (
    BuilderError,
    ClientClosedError,
    ConnectError,
    ConnectTimeoutError,
    PoolTimeoutError,
    ReadTimeoutError,
    StatusError,
)
from pyreqwest.http import HeaderMap, Url
from pyreqwest.request import BaseRequestBuilder, ConsumedRequest, Request, RequestBuilder
from pyreqwest.response import BaseResponse, Response, ResponseBodyReader

from .servers.echo_server import EchoServer
from .servers.server import find_free_port


async def test_base_url(echo_server: EchoServer):
    async def echo_path(client: Client, path: str) -> str:
        resp = await (await client.get(path).build().send()).json()
        return str(resp["path"])

    async with ClientBuilder().base_url(echo_server.url).error_for_status(True).build() as client:
        assert await echo_path(client, "") == "/"
        assert await echo_path(client, "/") == "/"
        assert await echo_path(client, "test") == "/test"
        assert await echo_path(client, "/test") == "/test"
        assert await echo_path(client, "test/") == "/test/"
        assert await echo_path(client, "/test/") == "/test/"

    async with ClientBuilder().base_url(echo_server.url / "mid/").error_for_status(True).build() as client:
        assert await echo_path(client, "") == "/mid/"
        assert await echo_path(client, "/") == "/"
        assert await echo_path(client, "test") == "/mid/test"
        assert await echo_path(client, "/test") == "/test"
        assert await echo_path(client, "test/") == "/mid/test/"
        assert await echo_path(client, "/test/") == "/test/"

    with pytest.raises(ValueError, match="base_url must end with a trailing slash '/'"):
        ClientBuilder().base_url(echo_server.url / "bad")


@pytest.mark.parametrize("value", [True, False])
async def test_error_for_status(echo_server: EchoServer, value: bool):
    url = echo_server.url.with_query({"status": 400})

    async with ClientBuilder().error_for_status(value).build() as client:
        req = client.get(url).build()
        if value:
            with pytest.raises(StatusError) as e:
                await req.send()
            assert e.value.details
            assert e.value.details["status"] == 400
        else:
            assert (await req.send()).status == 400


@pytest.mark.parametrize("value", [1, 2, None])
async def test_max_connections_pool_timeout(echo_server: EchoServer, value: int | None):
    url = echo_server.url.with_query({"sleep_start": 0.1})

    builder = ClientBuilder().max_connections(value).pool_timeout(timedelta(seconds=0.05)).error_for_status(True)

    async with builder.build() as client:
        coros = [client.get(url).build().send() for _ in range(2)]
        if value == 1:
            with pytest.raises(PoolTimeoutError) as e:
                await asyncio.gather(*coros)
            assert isinstance(e.value, TimeoutError)
        else:
            await asyncio.gather(*coros)


@pytest.mark.parametrize("value", [0.05, 0.2, None])
@pytest.mark.parametrize("sleep_kind", ["sleep_start", "sleep_body"])
async def test_timeout(echo_server: EchoServer, value: float | None, sleep_kind: str):
    url = echo_server.url.with_query({sleep_kind: 0.1})

    builder = ClientBuilder().error_for_status(True)
    if value is not None:
        builder = builder.timeout(timedelta(seconds=value))

    async with builder.build() as client:
        req = client.get(url).build()
        if value and value < 0.2:
            exc = ConnectTimeoutError if sleep_kind == "sleep_start" else ReadTimeoutError
            with pytest.raises(exc) as e:
                await req.send()
            assert isinstance(e.value, TimeoutError)
        else:
            await req.send()


async def test_no_connection():
    port = find_free_port()
    async with ClientBuilder().error_for_status(True).build() as client:
        req = client.get(Url(f"http://localhost:{port}")).build()
        with pytest.raises(ConnectError) as e:
            await req.send()
        assert e.value.details and {"message": "tcp connect error"} in (e.value.details["causes"] or [])


async def test_user_agent(echo_server: EchoServer):
    async with ClientBuilder().user_agent("ua-test").error_for_status(True).build() as client:
        res = await (await client.get(echo_server.url).build().send()).json()
        assert ["user-agent", "ua-test"] in res["headers"]


@pytest.mark.parametrize(
    "value",
    [HeaderMap({"X-Test": "foobar"}), {"X-Test": "foobar"}, HeaderMap([("X-Test", "foo"), ("X-Test", "bar")])],
)
async def test_default_headers__good(echo_server: EchoServer, value: Mapping[str, str]):
    async with ClientBuilder().default_headers(value).error_for_status(True).build() as client:
        res = await (await client.get(echo_server.url).build().send()).json()
        for name, v in value.items():
            assert [name.lower(), v] in res["headers"]


async def test_default_headers__bad():
    with pytest.raises(TypeError, match="argument 'headers': 'str' object cannot be converted to 'PyTuple'"):
        ClientBuilder().default_headers(["foo"])  # type: ignore[list-item]
    with pytest.raises(TypeError, match="argument 'headers': 'int' object cannot be converted to 'PyString'"):
        ClientBuilder().default_headers({"X-Test": 123})  # type: ignore[dict-item]
    with pytest.raises(TypeError, match="argument 'headers': 'str' object cannot be converted to 'PyTuple'"):
        ClientBuilder().default_headers("bad")  # type: ignore[arg-type]
    with pytest.raises(ValueError, match="invalid HTTP header name"):
        ClientBuilder().default_headers({"X-Test\n": "foo"})
    with pytest.raises(ValueError, match="failed to parse header value"):
        ClientBuilder().default_headers({"X-Test": "bad\n"})


async def test_response_compression(echo_server: EchoServer):
    async with ClientBuilder().error_for_status(True).build() as client:
        res = await (await client.get(echo_server.url).build().send()).json()
        assert ["accept-encoding", "gzip, br, zstd, deflate"] in res["headers"]
        url = echo_server.url.with_query({"compress": "gzip"})
        resp = await client.get(url).build().send()
        assert resp.headers["x-content-encoding"] == "gzip"
        assert await resp.json()

    async with ClientBuilder().gzip(False).error_for_status(True).build() as client:
        res = await (await client.get(echo_server.url).build().send()).json()
        assert ["accept-encoding", "br, zstd, deflate"] in res["headers"]


@pytest.mark.parametrize("str_url", [False, True])
async def test_http_methods(echo_server: EchoServer, str_url: bool):
    url = str(echo_server.url) if str_url else echo_server.url
    async with ClientBuilder().error_for_status(True).build() as client:
        async with client.get(url).build_streamed() as response:
            assert (await response.json())["method"] == "GET"
            assert (await response.json())["scheme"] == "http"
        async with client.post(url).build_streamed() as response:
            assert (await response.json())["method"] == "POST"
        async with client.put(url).build_streamed() as response:
            assert (await response.json())["method"] == "PUT"
        async with client.patch(url).build_streamed() as response:
            assert (await response.json())["method"] == "PATCH"
        async with client.delete(url).build_streamed() as response:
            assert (await response.json())["method"] == "DELETE"
        async with client.request("QUERY", url).build_streamed() as response:
            assert (await response.json())["method"] == "QUERY"


async def test_use_after_close(echo_server: EchoServer):
    async with ClientBuilder().error_for_status(True).build() as client:
        assert (await client.get(echo_server.url).build().send()).status == 200
    req = client.get(echo_server.url).build()
    with pytest.raises(ClientClosedError, match="Client was closed"):
        await req.send()

    client = ClientBuilder().error_for_status(True).build()
    await client.close()
    req = client.get(echo_server.url).build()
    with pytest.raises(ClientClosedError, match="Client was closed"):
        await req.send()


async def test_close_in_request(echo_server: EchoServer):
    url = echo_server.url.with_query({"sleep_start": 1})

    async with ClientBuilder().error_for_status(True).build() as client:
        req = client.get(url).build()
        task = asyncio.create_task(req.send())
        await asyncio.sleep(0.05)
        await client.close()
        with pytest.raises(ClientClosedError, match="Client was closed"):
            await task


async def test_builder_use_after_build():
    builder = ClientBuilder()
    client = builder.build()
    with pytest.raises(RuntimeError, match="Client was already built"):
        builder.error_for_status(True)
    with pytest.raises(RuntimeError, match="Client was already built"):
        builder.build()
    await client.close()


async def test_https_only(echo_server: EchoServer):
    async with ClientBuilder().https_only(True).error_for_status(True).build() as client:
        req = client.get(echo_server.url).build()
        with pytest.raises(BuilderError, match="builder error") as e:
            await req.send()
        assert e.value.details and {"message": "URL scheme is not allowed"} in (e.value.details["causes"] or [])


async def test_https(https_echo_server: EchoServer, cert_authority: trustme.CA):
    cert_pem = cert_authority.cert_pem.bytes()
    builder = ClientBuilder().add_root_certificate_pem(cert_pem).https_only(True).error_for_status(True)
    async with builder.build() as client:
        resp = await client.get(https_echo_server.url).build().send()
        assert (await resp.json())["scheme"] == "https"

    cert_der = x509.load_pem_x509_certificate(cert_pem).public_bytes(serialization.Encoding.DER)
    builder = ClientBuilder().add_root_certificate_der(cert_der).https_only(True).error_for_status(True)
    async with builder.build() as client:
        resp = await client.get(https_echo_server.url).build().send()
        assert (await resp.json())["scheme"] == "https"


async def test_https__no_trust(https_echo_server: EchoServer):
    builder = ClientBuilder().https_only(True).error_for_status(True)
    async with builder.build() as client:
        req = client.get(https_echo_server.url).build()
        with pytest.raises(ConnectError) as e:
            await req.send()
        assert e.value.details
        assert {"message": "invalid peer certificate: UnknownIssuer"} in (e.value.details["causes"] or [])


async def test_https__accept_invalid_certs(https_echo_server: EchoServer):
    builder = ClientBuilder().danger_accept_invalid_certs(True).https_only(True).error_for_status(True)
    async with builder.build() as client:
        resp = await client.get(https_echo_server.url).build().send()
        assert (await resp.json())["scheme"] == "https"


@pytest.mark.parametrize("returns", [bytes, bytearray, memoryview])
async def test_json_dumps_callback(echo_server: EchoServer, returns: type[bytes | bytearray | memoryview]):
    called = 0

    def custom_dumps(ctx: JsonDumpsContext) -> bytes | bytearray | memoryview:
        nonlocal called
        called += 1
        assert isinstance(ctx.data, dict)
        return returns(json.dumps({**ctx.data, "test": 1}).encode())

    async with ClientBuilder().json_handler(dumps=custom_dumps).error_for_status(True).build() as client:
        assert called == 0
        req = client.post(echo_server.url).body_json({"original": "data"})
        assert called == 1
        resp = await req.build().send()
        assert (await resp.json())["body_parts"] == ['{"original": "data", "test": 1}']
        assert called == 1


async def test_json_loads_callback(echo_server: EchoServer):
    called = 0

    async def custom_loads(ctx: JsonLoadsContext) -> Any:
        nonlocal called
        called += 1
        assert ctx.headers["Content-Type"] == "application/json"
        assert ctx.extensions == {"my_ext": "foo"}
        content = (await ctx.body_reader.bytes()).to_bytes()

        assert type(ctx.body_reader) is ResponseBodyReader
        assert type(ctx.headers) is HeaderMap
        assert type(ctx.extensions) is dict

        return {**json.loads(content), "test": "bar"}

    async with ClientBuilder().json_handler(loads=custom_loads).error_for_status(True).build() as client:
        resp = await client.get(echo_server.url).extensions({"my_ext": "foo"}).build().send()
        assert called == 0
        res = await resp.json()
        assert called == 1
        assert res.pop("test") == "bar"
        assert json.loads((await resp.bytes()).to_bytes()) == res
        assert (await resp.json()) == {**res, "test": "bar"}
        assert called == 2


async def test_different_runtimes(echo_server: EchoServer):
    rt1 = Runtime()
    rt2 = Runtime()

    client1 = ClientBuilder().runtime(rt1).error_for_status(True).build()
    client2 = ClientBuilder().runtime(rt2).error_for_status(True).build()

    await client1.get(echo_server.url).build().send()
    await client2.get(echo_server.url).build().send()

    await rt1.close()

    with pytest.raises(ClientClosedError, match="Runtime was closed"):
        await client1.get(echo_server.url).build().send()
    await client2.get(echo_server.url).build().send()

    del rt2
    with pytest.raises(ClientClosedError, match="Runtime was closed"):
        await client2.get(echo_server.url).build().send()


async def test_types(echo_server: EchoServer) -> None:
    builder = ClientBuilder().error_for_status(True)
    assert type(builder) is ClientBuilder and isinstance(builder, BaseClientBuilder)
    client = builder.build()
    assert type(client) is Client and isinstance(client, BaseClient)
    req_builder = client.get(echo_server.url)
    assert type(req_builder) is RequestBuilder and isinstance(req_builder, BaseRequestBuilder)
    req = req_builder.build()
    assert type(req) is ConsumedRequest and isinstance(req, Request)
    resp = await req.send()
    assert type(resp) is Response and isinstance(resp, BaseResponse)
