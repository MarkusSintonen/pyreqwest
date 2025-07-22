import pytest
import trustme

from pyreqwest.client import ClientBuilder
from pyreqwest.exceptions import ConnectError
from pyreqwest.proxy import Proxy
from .servers.echo_server import EchoServer


@pytest.mark.parametrize("proxy_type", ["http", "all"])
async def test_proxy_simple(
    https_echo_server: EchoServer,
    https_echo_server_proxy: EchoServer,
    cert_authority: trustme.CA,
    proxy_type: str
):
    if proxy_type == "http":
        proxy = Proxy.http(https_echo_server.address)
    else:
        assert proxy_type == "all"
        proxy = Proxy.all(https_echo_server.address)

    cert_pem = cert_authority.cert_pem.bytes()
    async with ClientBuilder().proxy(proxy).add_root_certificate_pem(cert_pem).error_for_status(True).build() as client:
        resp = await client.get(f"http://unknown.example/test").build_consumed().send()
        assert (await resp.json())['scheme'] == "https"
        assert ['host', 'unknown.example'] in (await resp.json())['headers']

    # no proxy fails
    async with ClientBuilder().add_root_certificate_pem(cert_pem).error_for_status(True).build() as client:
        req = client.get("http://unknown.example/test").build_consumed()
        with pytest.raises(ConnectError) as e:
            await req.send()
        assert {'message': 'dns error'} in e.value.details["causes"]


async def test_proxy_custom(echo_server: EchoServer):
    proxy = Proxy.custom(lambda url: echo_server.address if "unknown.example" in str(url) else None)

    async with ClientBuilder().proxy(proxy).error_for_status(True).build() as client:
        resp = await client.get(f"http://unknown.example/").build_consumed().send()
        assert (await resp.json())['scheme'] == "http"
        assert ['host', 'unknown.example'] in (await resp.json())['headers']

        with pytest.raises(ConnectError):
            await client.get(f"http://unknown2.example/").build_consumed().send()  # not captured


async def test_proxy_headers(echo_server: EchoServer):
    proxy = Proxy.custom(lambda _: echo_server.address).headers({"X-Custom-Header": "CustomValue"})

    async with ClientBuilder().proxy(proxy).error_for_status(True).build() as client:
        req = client.get(f"http://unknown.example/").build_consumed()
        assert req.copy_headers() == {}
        resp = await req.send()
        assert ['x-custom-header', 'CustomValue'] in (await resp.json())['headers']
