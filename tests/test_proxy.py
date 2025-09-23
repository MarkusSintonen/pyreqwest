from collections.abc import Callable

import pytest
import trustme
from pyreqwest.client import ClientBuilder
from pyreqwest.exceptions import ConnectError, RequestPanicError
from pyreqwest.http import Url
from pyreqwest.proxy import ProxyBuilder

from tests.servers.server_subprocess import SubprocessServer


def test_init():
    ProxyBuilder.http("http://proxy.example.com")
    ProxyBuilder.https("http://proxy.example.com")
    ProxyBuilder.all("http://proxy.example.com")
    ProxyBuilder.custom(lambda _: "http://proxy.example.com")


@pytest.mark.parametrize("proxy_type", ["http", "all"])
async def test_proxy_simple(
    https_echo_server: SubprocessServer,
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
        resp = await client.get("http://foo.invalid/test").build().send()
        assert (await resp.json())["scheme"] == "https"
        assert ["host", "foo.invalid"] in (await resp.json())["headers"]

    # no proxy fails
    async with ClientBuilder().add_root_certificate_pem(cert_pem).error_for_status(True).build() as client:
        req = client.get("http://foo.invalid/test").build()
        with pytest.raises(ConnectError) as e:
            await req.send()
        assert e.value.details and {"message": "dns error"} in e.value.details["causes"]


async def test_proxy_custom(echo_server: SubprocessServer):
    def proxy_func(url: Url) -> Url | str | None:
        return echo_server.url if "foo.invalid" in str(url) else None

    proxy = ProxyBuilder.custom(proxy_func)

    async with ClientBuilder().proxy(proxy).error_for_status(True).build() as client:
        resp = await client.get("http://foo.invalid/").build().send()
        assert (await resp.json())["scheme"] == "http"
        assert ["host", "foo.invalid"] in (await resp.json())["headers"]

        with pytest.raises(ConnectError):
            await client.get("http://foo2.invalid/").build().send()  # not captured


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
        req = client.get("http://foo.invalid/").build()
        with pytest.raises(RequestPanicError) as e:
            await req.send()
        assert e.value.details and expect_cause in e.value.details["causes"]


async def test_proxy_headers(echo_server: SubprocessServer):
    proxy = ProxyBuilder.custom(lambda _: echo_server.url).headers({"X-Custom-Header": "CustomValue"})

    async with ClientBuilder().proxy(proxy).error_for_status(True).build() as client:
        req = client.get("http://foo.invalid/").build()
        assert req.headers == {}
        resp = await req.send()
        assert ["x-custom-header", "CustomValue"] in (await resp.json())["headers"]


async def test_basic_auth(echo_server: SubprocessServer):
    proxy = ProxyBuilder.http(echo_server.url).basic_auth("user", "pass")

    async with ClientBuilder().proxy(proxy).error_for_status(True).build() as client:
        resp = await client.get("http://foo.invalid/").build().send()
        assert dict((await resp.json())["headers"])["proxy-authorization"].startswith("Basic ")


async def test_custom_auth(echo_server: SubprocessServer):
    proxy = ProxyBuilder.http(echo_server.url).custom_http_auth("auth_value")

    async with ClientBuilder().proxy(proxy).error_for_status(True).build() as client:
        resp = await client.get("http://foo.invalid/").build().send()
        assert dict((await resp.json())["headers"])["proxy-authorization"] == "auth_value"


async def test_no_proxy(echo_server: SubprocessServer):
    proxy = ProxyBuilder.http(echo_server.url).no_proxy("noproxy.invalid, noproxy2.invalid")

    async with ClientBuilder().proxy(proxy).error_for_status(True).build() as client:
        with pytest.raises(ConnectError):
            await client.get("http://noproxy.invalid/").build().send()
        with pytest.raises(ConnectError):
            await client.get("http://noproxy2.invalid/").build().send()

        resp = await client.get("http://doproxy.invalid/").build().send()
        assert ["host", "doproxy.invalid"] in (await resp.json())["headers"]
