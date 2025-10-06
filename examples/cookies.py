"""Auth usage.

Run directly:
    uv run python -m examples.cookies
"""

import asyncio
import sys
from typing import Any

from pyreqwest.client import ClientBuilder
from pyreqwest.cookie import CookieStore

from ._utils import httpbin_url, run_examples


async def example_cookies() -> None:
    """Cookie example"""
    async with ClientBuilder().default_cookie_store(True).error_for_status(True).build() as client:
        resp = (
            await client.get(httpbin_url() / "cookies/set").query({"cookie1": "val1", "cookie2": "val2"}).build().send()
        )
        cookies = _get_cookies(await resp.json())
        assert cookies["cookie1"] == "val1" and cookies["cookie2"] == "val2"
        print({"set_cookies": cookies})

        resp = await client.get(httpbin_url() / "cookies").build().send()
        cookies = _get_cookies(await resp.json())
        assert cookies["cookie1"] == "val1" and cookies["cookie2"] == "val2"
        print({"sent_cookies": cookies})


async def example_cookie_store() -> None:
    """Access cookies in cookie store"""
    store = CookieStore()
    async with ClientBuilder().cookie_provider(store).error_for_status(True).build() as client:
        await client.get(httpbin_url() / "cookies/set").query({"cookie1": "val1"}).build().send()

        resp = await client.get(httpbin_url() / "cookies").build().send()
        cookies = _get_cookies(await resp.json())
        assert cookies["cookie1"] == "val1"
        print({"sent_cookies": cookies})

        assert [(c.name, c.value) for c in store.get_all_any()] == [("cookie1", "val1")]
        print({"cookie_store": store.get_all_any()})


def _get_cookies(resp: dict[str, Any]) -> dict[str, Any]:
    cookies = resp.get("cookies", resp)  # go httpbin does not always wrap cookies in "cookies"
    assert isinstance(cookies, dict)
    return cookies


if __name__ == "__main__":
    asyncio.run(run_examples(sys.modules[__name__]))
