import asyncio
import json
from collections.abc import Mapping
from datetime import timedelta
from pathlib import Path
from typing import Any

import pytest
import trustme
from cryptography import x509
from cryptography.hazmat.primitives import serialization
from pyreqwest.client import BaseClient, BaseClientBuilder, Client, ClientBuilder, Runtime
from pyreqwest.client.types import JsonDumpsContext, JsonLoadsContext
from pyreqwest.exceptions import (
    BodyDecodeError,
    BuilderError,
    ClientClosedError,
    ConnectError,
    ConnectTimeoutError,
    DecodeError,
    PoolTimeoutError,
    ReadError,
    ReadTimeoutError,
    RedirectError,
    StatusError,
)
from pyreqwest.http import HeaderMap, Url
from pyreqwest.request import BaseRequestBuilder, ConsumedRequest, Request, RequestBuilder
from pyreqwest.response import BaseResponse, Response, ResponseBodyReader

from tests.utils import IS_CI

from .servers.server import find_free_port
from .servers.server_subprocess import SubprocessServer


async def test_base_url(echo_server: SubprocessServer):
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
async def test_error_for_status(echo_server: SubprocessServer, value: bool):
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
@pytest.mark.parametrize("timeout_val", [timedelta(seconds=0.05), None])
async def test_max_connections_pool_timeout(
    echo_server: SubprocessServer, value: int | None, timeout_val: timedelta | None
):
    url = echo_server.url.with_query({"sleep_start": 0.1})

    builder = ClientBuilder().max_connections(value).error_for_status(True)
    if timeout_val:
        builder = builder.pool_timeout(timeout_val)

    async with builder.build() as client:
        coros = [client.get(url).build().send() for _ in range(2)]
        if value == 1 and timeout_val:
            with pytest.raises(PoolTimeoutError) as e:
                await asyncio.gather(*coros)
            assert isinstance(e.value, TimeoutError)
        else:
            await asyncio.gather(*coros)


async def test_max_connections_full_timeout(echo_server: SubprocessServer):
    timeout = 0.5 if IS_CI else 0.05
    sleep = 0.1 if IS_CI else 0.01

    url = echo_server.url.with_query({"sleep_start": sleep})

    builder = ClientBuilder().max_connections(1).timeout(timedelta(seconds=timeout)).error_for_status(True)

    async with builder.build() as client:
        coros = [client.get(url).build().send() for _ in range(10)]
        with pytest.raises(PoolTimeoutError) as e:
            await asyncio.gather(*coros)
        assert isinstance(e.value, TimeoutError)


@pytest.mark.parametrize("timeout_value", [0.05, 0.2, None])
@pytest.mark.parametrize("sleep_kind", ["sleep_start", "sleep_body"])
@pytest.mark.parametrize("timeout_kind", ["total", "read", "connect"])
async def test_timeout(echo_server: SubprocessServer, timeout_value: float | None, sleep_kind: str, timeout_kind: str):
    timeout = timeout_value * 10 if IS_CI and timeout_value else timeout_value
    sleep = 1 if IS_CI else 0.1

    url = echo_server.url.with_query({sleep_kind: sleep})

    builder = ClientBuilder().error_for_status(True)
    if timeout is not None:
        if timeout_kind == "total":
            builder = builder.timeout(timedelta(seconds=timeout))
        elif timeout_kind == "read":
            builder = builder.read_timeout(timedelta(seconds=timeout))
        else:
            assert timeout_kind == "connect"
            builder = builder.connect_timeout(timedelta(seconds=timeout))

    async with builder.build() as client:
        req = client.get(url).build()
        if timeout_value and timeout_value < 0.2 and timeout_kind != "connect":
            exc = ConnectTimeoutError if sleep_kind == "sleep_start" else ReadTimeoutError
            with pytest.raises(exc) as e:
                await req.send()
            assert isinstance(e.value, TimeoutError)
        else:
            await req.send()


async def test_connection_failure():
    port = find_free_port()
    async with ClientBuilder().error_for_status(True).build() as client:
        req = client.get(Url(f"http://localhost:{port}")).build()
        with pytest.raises(ConnectError) as e:
            await req.send()
        assert e.value.details and {"message": "tcp connect error"} in (e.value.details["causes"] or [])


async def test_connection_failure__while_client_send(echo_server: SubprocessServer):
    killed = False

    async def stream_gen() -> Any:
        nonlocal killed
        yield b"test"
        killed = True
        await echo_server.kill()
        yield b"test2"

    async with ClientBuilder().error_for_status(True).build() as client:
        req = client.post(echo_server.url).body_stream(stream_gen()).build()
        with pytest.raises(ConnectError, match="connection error"):
            await req.send()
        assert killed


async def test_connection_failure__while_client_read(echo_body_parts_server: SubprocessServer):
    async def stream_gen() -> Any:
        for i in range(10):
            await asyncio.sleep(0.1)
            yield str(i).encode() * 65536

    async with (
        ClientBuilder().error_for_status(True).build() as client,
        client.post(echo_body_parts_server.url)
        .body_stream(stream_gen())
        .streamed_read_buffer_limit(0)
        .build_streamed() as resp,
    ):
        await resp.body_reader.read(65536)

        await echo_body_parts_server.kill()

        with pytest.raises(ReadError, match="response body connection error") as e:  # noqa: PT012
            while await resp.body_reader.read(65536):
                pass
        assert e.value.details and {"message": "error reading a body from connection"} in e.value.details["causes"]


async def test_too_big_response_header(echo_server: SubprocessServer):
    url = echo_server.url.with_query({"header_repeat": "a:1000000"})

    async with ClientBuilder().error_for_status(True).build() as client:
        req = client.get(url).build()
        with pytest.raises(DecodeError, match="error decoding response") as e:
            await req.send()
        assert type(e.value) is DecodeError
        assert e.value.details and {"message": "message head is too large"} in e.value.details["causes"]


async def test_user_agent(echo_server: SubprocessServer):
    async with ClientBuilder().error_for_status(True).build() as client:
        res = await (await client.get(echo_server.url).build().send()).json()
        assert ["user-agent", "python-pyreqwest/1.0.0"] in res["headers"]

    async with ClientBuilder().user_agent("ua-test").error_for_status(True).build() as client:
        res = await (await client.get(echo_server.url).build().send()).json()
        assert ["user-agent", "ua-test"] in res["headers"]


@pytest.mark.parametrize(
    "value",
    [HeaderMap({"X-Test": "foobar"}), {"X-Test": "foobar"}, HeaderMap([("X-Test", "foo"), ("X-Test", "bar")])],
)
async def test_default_headers__good(echo_server: SubprocessServer, value: Mapping[str, str]):
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


async def test_response_compression(echo_server: SubprocessServer):
    async with ClientBuilder().error_for_status(True).build() as client:
        res = await (await client.get(echo_server.url).build().send()).json()
        assert ["accept-encoding", "gzip, br, zstd, deflate"] in res["headers"]
        url = echo_server.url.with_query({"compress": "gzip"})
        resp = await client.get(url).build().send()
        assert resp.headers["x-content-encoding"] == "gzip"
        assert await resp.json()

        with pytest.raises(BodyDecodeError, match="error decoding body") as e:
            await client.get(echo_server.url.with_query({"compress": "gzip_invalid"})).build().send()
        assert e.value.details and {"message": "Invalid gzip header"} in e.value.details["causes"]

    async with ClientBuilder().gzip(False).error_for_status(True).build() as client:
        res = await (await client.get(echo_server.url).build().send()).json()
        assert ["accept-encoding", "br, zstd, deflate"] in res["headers"]


@pytest.mark.parametrize("str_url", [False, True])
async def test_http_methods(echo_server: SubprocessServer, str_url: bool):
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
        async with client.head(url).build_streamed() as response:
            assert response.headers["content-type"] == "application/json"
        async with client.request("QUERY", url).build_streamed() as response:
            assert (await response.json())["method"] == "QUERY"


async def test_use_after_close(echo_server: SubprocessServer):
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


async def test_close_in_request(echo_server: SubprocessServer):
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


async def test_https_only(echo_server: SubprocessServer):
    async with ClientBuilder().https_only(True).error_for_status(True).build() as client:
        req = client.get(echo_server.url).build()
        with pytest.raises(BuilderError, match="builder error") as e:
            await req.send()
        assert e.value.details and {"message": "URL scheme is not allowed"} in (e.value.details["causes"] or [])


async def test_https(https_echo_server: SubprocessServer, cert_authority: trustme.CA):
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


async def test_https__no_trust(https_echo_server: SubprocessServer):
    builder = ClientBuilder().https_only(True).error_for_status(True)
    async with builder.build() as client:
        req = client.get(https_echo_server.url).build()
        with pytest.raises(ConnectError) as e:
            await req.send()
        assert e.value.details
        assert {"message": "invalid peer certificate: UnknownIssuer"} in (e.value.details["causes"] or [])


async def test_https__accept_invalid_certs(https_echo_server: SubprocessServer):
    builder = ClientBuilder().danger_accept_invalid_certs(True).https_only(True).error_for_status(True)
    async with builder.build() as client:
        resp = await client.get(https_echo_server.url).build().send()
        assert (await resp.json())["scheme"] == "https"


@pytest.mark.parametrize("returns", [bytes, bytearray, memoryview])
async def test_json_dumps_callback(echo_server: SubprocessServer, returns: type[bytes | bytearray | memoryview]):
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

    async def bad_dumps(_ctx: JsonDumpsContext) -> bytes | bytearray | memoryview:
        raise RuntimeError("should not be called")

    with pytest.raises(ValueError, match="dumps must be a sync function"):
        ClientBuilder().json_handler(dumps=bad_dumps)  # type: ignore[arg-type]

    with pytest.raises(ValueError, match="Expected a callable"):
        ClientBuilder().json_handler(dumps="bad")  # type: ignore[arg-type]


async def test_json_loads_callback(echo_server: SubprocessServer):
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

    def bad_loads(_ctx: JsonLoadsContext) -> Any:
        raise RuntimeError("should not be called")

    with pytest.raises(ValueError, match="loads must be an async function"):
        ClientBuilder().json_handler(loads=bad_loads)

    with pytest.raises(ValueError, match="Expected a callable"):
        ClientBuilder().json_handler(loads="bad")  # type: ignore[arg-type]


async def test_various_builder_functions(
    https_echo_server: SubprocessServer,
    echo_server: SubprocessServer,
    cert_authority: trustme.CA,
    localhost_cert: trustme.LeafCert,
):
    client = (
        ClientBuilder()
        .default_cookie_store(True)
        .brotli(False)
        .zstd(False)
        .deflate(False)
        .max_redirects(1)
        .referer(True)
        .no_proxy()
        .pool_idle_timeout(timedelta(seconds=1))
        .pool_max_idle_per_host(1)
        .http1_lower_case_headers()
        .http1_allow_obsolete_multiline_headers_in_responses(True)
        .http1_ignore_invalid_headers_in_responses(True)
        .http1_allow_spaces_after_header_name_in_responses(True)
        .http1_only()
        .tcp_nodelay(True)
        .local_address("127.0.0.1")
        .tcp_keepalive(timedelta(seconds=1))
        .tcp_keepalive_interval(timedelta(seconds=1))
        .tcp_keepalive_retries(1)
        .tls_built_in_root_certs(True)
        .danger_accept_invalid_hostnames(True)
        .tls_sni(True)
        .min_tls_version("TLSv1.0")
        .max_tls_version("TLSv1.3")
        .add_root_certificate_pem(cert_authority.cert_pem.bytes())
        .identity_pem(localhost_cert.private_key_and_cert_chain_pem.bytes())
        .build()
    )
    async with client:
        resp = await client.get(https_echo_server.url).build().send()
        assert resp.status == 200

    async with ClientBuilder().http09_responses().error_for_status(True).build() as client:
        await client.get(echo_server.url).build().send()

    ClientBuilder().add_crl_pem((Path(__file__).parent / "samples" / "crl.pem").read_bytes())


async def test_http2_builder_functions(https_echo_server: SubprocessServer, cert_authority: trustme.CA):
    client = (
        ClientBuilder()
        .add_root_certificate_pem(cert_authority.cert_pem.bytes())
        .http2_prior_knowledge()
        .http2_adaptive_window(True)
        .http2_initial_connection_window_size(65535)
        .http2_initial_stream_window_size(65535)
        .http2_max_frame_size(65535)
        .http2_max_header_list_size(16384)
        .http2_keep_alive_interval(timedelta(seconds=1))
        .http2_keep_alive_timeout(timedelta(seconds=1))
        .http2_keep_alive_while_idle(True)
        .build()
    )
    async with client:
        resp = await client.get(https_echo_server.url).build().send()
        assert resp.status == 200 and resp.version == "HTTP/2.0"


async def test_resolve(echo_server: SubprocessServer):
    assert echo_server.url.port
    async with ClientBuilder().resolve("foobar.local", "127.0.0.1", echo_server.url.port).build() as client:
        resp = await client.get("http://foobar.local").build().send()
        assert resp.status == 200


async def test_max_redirects(echo_server: SubprocessServer):
    url = echo_server.url.with_query({"status": 302, "header_location": "/redirect"})

    async with ClientBuilder().max_redirects(1).error_for_status(True).build() as client:
        req = client.get(url).build()
        resp = await req.send()
        assert (await resp.json())["path"] == "/redirect"
        assert resp.status == 200

    async with ClientBuilder().max_redirects(0).error_for_status(True).build() as client:
        req = client.get(url).build()
        with pytest.raises(RedirectError, match="error following redirect") as e:
            await req.send()
        assert e.value.details and {"message": "too many redirects"} in e.value.details["causes"]


def test_bad_tls_version():
    with pytest.raises(ValueError, match="Invalid TLS version"):
        ClientBuilder().min_tls_version("bad")  # type: ignore[arg-type]
    with pytest.raises(ValueError, match="Invalid TLS version"):
        ClientBuilder().max_tls_version("bad")  # type: ignore[arg-type]


async def test_different_runtimes(echo_server: SubprocessServer):
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


async def test_types(echo_server: SubprocessServer) -> None:
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
