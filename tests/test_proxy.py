from collections.abc import Callable

import pytest
import trustme
from pyreqwest.client import ClientBuilder
from pyreqwest.exceptions import ConnectError, RequestPanicError
from pyreqwest.http import Url
from pyreqwest.proxy import ProxyBuilder

from .servers.echo_server import EchoServer


@pytest.mark.parametrize("proxy_type", ["http", "all"])
async def test_proxy_simple(
    https_echo_server: EchoServer,
    cert_authority: trustme.CA,
    proxy_type: str,
):
    if proxy_type == "http":
        proxy = ProxyBuilder.http(https_echo_server.url)
    else:
        assert proxy_type == "all"
        proxy = ProxyBuilder.all(https_echo_server.url)

    cert_pem = cert_authority.cert_pem.bytes()
    async with ClientBuilder().proxy(proxy).add_root_certificate_pem(cert_pem).error_for_status(True).build() as client:
        resp = await client.get("http://foo.invalid/test").build_consumed().send()
        assert (await resp.json())["scheme"] == "https"
        assert ["host", "foo.invalid"] in (await resp.json())["headers"]

    # no proxy fails
    async with ClientBuilder().add_root_certificate_pem(cert_pem).error_for_status(True).build() as client:
        req = client.get("http://foo.invalid/test").build_consumed()
        with pytest.raises(ConnectError) as e:
            await req.send()
        assert e.value.details and {"message": "dns error"} in e.value.details["causes"]


async def test_proxy_custom(echo_server: EchoServer):
    def proxy_func(url: Url) -> Url | str | None:
        return echo_server.url if "foo.invalid" in str(url) else None

    proxy = ProxyBuilder.custom(proxy_func)

    async with ClientBuilder().proxy(proxy).error_for_status(True).build() as client:
        resp = await client.get("http://foo.invalid/").build_consumed().send()
        assert (await resp.json())["scheme"] == "http"
        assert ["host", "foo.invalid"] in (await resp.json())["headers"]

        with pytest.raises(ConnectError):
            await client.get("http://foo2.invalid/").build_consumed().send()  # not captured


@pytest.mark.parametrize("case", ["raises", "bad_return"])
async def test_proxy_custom__fail(case: str):
    def proxy_func_raises(_url: Url) -> str | None:
        raise Exception("Custom error")

    def proxy_func_bad_return(_url: Url) -> str | None:
        return "not_a_valid_url"

    bad_fn: Callable[[Url], Url | str | None] = {
        "raises": proxy_func_raises,
        "bad_return": proxy_func_bad_return,
    }[case]
    expect_cause = {
        "raises": {"message": "Exception: Custom error"},
        "bad_return": {"message": "ValueError: relative URL without a base"},
    }[case]

    proxy = ProxyBuilder.custom(bad_fn)

    async with ClientBuilder().proxy(proxy).error_for_status(True).build() as client:
        req = client.get("http://foo.invalid/").build_consumed()
        with pytest.raises(RequestPanicError) as e:
            await req.send()
        assert e.value.details and expect_cause in e.value.details["causes"]


async def test_proxy_headers(echo_server: EchoServer):
    proxy = ProxyBuilder.custom(lambda _: echo_server.url).headers({"X-Custom-Header": "CustomValue"})

    async with ClientBuilder().proxy(proxy).error_for_status(True).build() as client:
        req = client.get("http://foo.invalid/").build_consumed()
        assert req.headers == {}
        resp = await req.send()
        assert ["x-custom-header", "CustomValue"] in (await resp.json())["headers"]
