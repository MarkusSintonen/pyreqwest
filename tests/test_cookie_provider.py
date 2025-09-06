from collections import defaultdict

import pytest
from pyreqwest.client import ClientBuilder
from pyreqwest.exceptions import RequestPanicError

from tests.servers.echo_server import EchoServer


class CookieProviderTest:
    def __init__(self) -> None:
        self.cookie_store: dict[str, list[str]] = defaultdict(list)

    def set_cookies(self, cookie_headers: list[str], url: str) -> None:
        self.cookie_store[url].extend(cookie_headers)

    def cookies(self, url: str) -> str | None:
        if url in self.cookie_store:
            return "; ".join(self.cookie_store[url])
        return None


async def test_cookie_provider(echo_server: EchoServer):
    echo_server.calls = 0
    provider = CookieProviderTest()

    async with ClientBuilder().cookie_provider(provider).build() as client:
        url1 = echo_server.url.with_query({"header_Set_Cookie": "cookiekey1=cookieval1"})
        await client.get(url1).build_consumed().send()
        assert provider.cookie_store == {str(url1): ["cookiekey1=cookieval1", "cookiekey1=cookieval1"]}

        assert echo_server.calls == 1

        url2 = echo_server.url.with_query({"header_Set_Cookie": "cookiekey2=cookieval2"})
        await client.get(url2).build_consumed().send()
        assert provider.cookie_store == {
            str(url1): ["cookiekey1=cookieval1", "cookiekey1=cookieval1"],
            str(url2): ["cookiekey2=cookieval2", "cookiekey2=cookieval2"],
        }


async def test_cookie_provider__error(echo_server: EchoServer):
    url = echo_server.url.with_query({"header_Set_Cookie": "cookiekey1=cookieval1"})

    class BadCookieProvider:
        def set_cookies(self, _cookie_headers: list[str], _url: str) -> None:
            raise RuntimeError("set_cookies error")

        def cookies(self, _url: str) -> str | None:
            raise RuntimeError("cookies error")

    async with ClientBuilder().cookie_provider(BadCookieProvider()).build() as client:
        req = client.get(url).build_consumed()
        with pytest.raises(RequestPanicError) as e:
            await req.send()
        assert e.value.details
        assert "RuntimeError('cookies error')" in e.value.details["causes"][0]
