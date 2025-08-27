import asyncio
from collections.abc import Mapping
from datetime import timedelta

import pytest
import trustme
from cryptography import x509
from cryptography.hazmat.primitives import serialization
from pyreqwest.client import ClientBuilder, Runtime
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

from .servers.echo_server import EchoServer
from .servers.server import find_free_port


@pytest.mark.parametrize("value", [True, False])
async def test_error_for_status(echo_server: EchoServer, value: bool):
    url = echo_server.url.with_query({"status": 400})

    async with ClientBuilder().error_for_status(value).build() as client:
        req = client.get(url).build_consumed()
        if value:
            with pytest.raises(StatusError) as e:
                await req.send()
            assert e.value.details["status"] == 400
        else:
            assert (await req.send()).status == 400


@pytest.mark.parametrize("value", [1, 2, None])
async def test_max_connections_pool_timeout(echo_server: EchoServer, value: int | None):
    url = echo_server.url.with_query({"sleep_start": 0.1})

    builder = ClientBuilder().max_connections(value).pool_timeout(timedelta(seconds=0.05)).error_for_status(True)

    async with builder.build() as client:
        coros = [client.get(url).build_consumed().send() for _ in range(2)]
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
        req = client.get(url).build_consumed()
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
        req = client.get(Url(f"http://localhost:{port}")).build_consumed()
        with pytest.raises(ConnectError) as e:
            await req.send()
        assert {"message": "tcp connect error"} in e.value.details["causes"]


async def test_user_agent(echo_server: EchoServer):
    async with ClientBuilder().user_agent("ua-test").error_for_status(True).build() as client:
        res = await (await client.get(echo_server.url).build_consumed().send()).json()
        assert ["user-agent", "ua-test"] in res["headers"]


@pytest.mark.parametrize(
    "value",
    [HeaderMap({"X-Test": "foobar"}), {"X-Test": "foobar"}, HeaderMap([("X-Test", "foo"), ("X-Test", "bar")])],
)
async def test_default_headers__good(echo_server: EchoServer, value: Mapping[str, str]):
    async with ClientBuilder().default_headers(value).error_for_status(True).build() as client:
        res = await (await client.get(echo_server.url).build_consumed().send()).json()
        for name, v in value.items():
            assert [name.lower(), v] in res["headers"]


async def test_default_headers__bad():
    with pytest.raises(TypeError, match="argument 'headers': 'str' object cannot be converted to 'PyTuple'"):
        ClientBuilder().default_headers(["foo"])
    with pytest.raises(TypeError, match="argument 'headers': 'int' object cannot be converted to 'PyString'"):
        ClientBuilder().default_headers({"X-Test": 123})
    with pytest.raises(TypeError, match="argument 'headers': 'str' object cannot be converted to 'PyTuple'"):
        ClientBuilder().default_headers("bad")
    with pytest.raises(ValueError, match="invalid HTTP header name"):
        ClientBuilder().default_headers({"X-Test\n": "foo"})
    with pytest.raises(ValueError, match="failed to parse header value"):
        ClientBuilder().default_headers({"X-Test": "bad\n"})


async def test_response_compression(echo_server: EchoServer):
    async with ClientBuilder().error_for_status(True).build() as client:
        res = await (await client.get(echo_server.url).build_consumed().send()).json()
        assert ["accept-encoding", "gzip, br, zstd, deflate"] in res["headers"]
        url = echo_server.url.with_query({"compress": "gzip"})
        resp = await client.get(url).build_consumed().send()
        assert resp.headers["x-content-encoding"] == "gzip"
        assert await resp.json()

    async with ClientBuilder().gzip(False).error_for_status(True).build() as client:
        res = await (await client.get(echo_server.url).build_consumed().send()).json()
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
        assert (await client.get(echo_server.url).build_consumed().send()).status == 200
    with pytest.raises(ClientClosedError, match="Client was closed"):
        await client.get(echo_server.url).build_consumed().send()

    client = ClientBuilder().error_for_status(True).build()
    await client.close()
    with pytest.raises(ClientClosedError, match="Client was closed"):
        await client.get(echo_server.url).build_consumed().send()


async def test_close_in_request(echo_server: EchoServer):
    url = echo_server.url.with_query({"sleep_start": 1})

    async with ClientBuilder().error_for_status(True).build() as client:
        req = client.get(url).build_consumed()
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
        req = client.get(echo_server.url).build_consumed()
        with pytest.raises(BuilderError, match="builder error") as e:
            await req.send()
        assert {"message": "URL scheme is not allowed"} in e.value.details["causes"]


async def test_https(https_echo_server: EchoServer, cert_authority: trustme.CA):
    cert_pem = cert_authority.cert_pem.bytes()
    builder = ClientBuilder().add_root_certificate_pem(cert_pem).https_only(True).error_for_status(True)
    async with builder.build() as client:
        resp = await client.get(https_echo_server.url).build_consumed().send()
        assert (await resp.json())["scheme"] == "https"

    cert_der = x509.load_pem_x509_certificate(cert_pem).public_bytes(serialization.Encoding.DER)
    builder = ClientBuilder().add_root_certificate_der(cert_der).https_only(True).error_for_status(True)
    async with builder.build() as client:
        resp = await client.get(https_echo_server.url).build_consumed().send()
        assert (await resp.json())["scheme"] == "https"


async def test_https__no_trust(https_echo_server: EchoServer):
    builder = ClientBuilder().https_only(True).error_for_status(True)
    async with builder.build() as client:
        req = client.get(https_echo_server.url).build_consumed()
        with pytest.raises(ConnectError) as e:
            await req.send()
        assert {"message": "invalid peer certificate: UnknownIssuer"} in e.value.details["causes"]


async def test_https__accept_invalid_certs(https_echo_server: EchoServer):
    builder = ClientBuilder().danger_accept_invalid_certs(True).https_only(True).error_for_status(True)
    async with builder.build() as client:
        resp = await client.get(https_echo_server.url).build_consumed().send()
        assert (await resp.json())["scheme"] == "https"


async def test_different_runtimes(echo_server: EchoServer):
    rt1 = Runtime()
    rt2 = Runtime()

    client1 = ClientBuilder().runtime(rt1).error_for_status(True).build()
    client2 = ClientBuilder().runtime(rt2).error_for_status(True).build()

    await client1.get(echo_server.url).build_consumed().send()
    await client2.get(echo_server.url).build_consumed().send()

    await rt1.close()

    with pytest.raises(ClientClosedError, match="Runtime was closed"):
        await client1.get(echo_server.url).build_consumed().send()
    await client2.get(echo_server.url).build_consumed().send()

    del rt2
    with pytest.raises(ClientClosedError, match="Runtime was closed"):
        await client2.get(echo_server.url).build_consumed().send()
