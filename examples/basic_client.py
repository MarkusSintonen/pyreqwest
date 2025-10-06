"""Basic usage.

Run directly:
    uv run python -m examples.basic_client
"""

import asyncio
import sys

from pyreqwest.client import ClientBuilder, SyncClientBuilder
from pyreqwest.exceptions import StatusError
from pyreqwest.http import Url

from ._utils import httpbin_url, run_examples


async def example_simple_get() -> None:
    """Simple GET"""
    async with ClientBuilder().error_for_status(True).build() as client:
        resp = await client.get(httpbin_url() / "get").query({"q": "pyreqwest"}).build().send()
        data = await resp.json()
        print({"args": data["args"], "url": data["url"], "status": resp.status})


def example_simple_get_sync() -> None:
    """Simple sync GET"""
    with SyncClientBuilder().error_for_status(True).build() as client:
        data = client.get(httpbin_url() / "get").query({"q": "pyreqwest"}).build().send().json()
        print({"args": data["args"], "url": data["url"]})


async def example_url_usage() -> None:
    """Url class usage (can be used to pass query params also)"""
    httpbin = Url(str(httpbin_url()))  # Construct from str
    with_path = httpbin / "get"  # Append path
    url = with_path.with_query({"q": "pyreqwest"})  # Add query params

    async with ClientBuilder().error_for_status(True).build() as client:
        resp = await client.get(url).build().send()
        data = await resp.json()
        print({"args": data["args"], "url": data["url"]})


async def example_error_for_status() -> None:
    """Error for status"""
    async with ClientBuilder().error_for_status(True).build() as client:
        req = client.get(httpbin_url() / "status/400").build()
        try:
            await req.send()
            raise RuntimeError("should have raised")
        except StatusError as e:
            print({"error": str(e)})

    # Does not raise if error_for_status is False (default)
    async with ClientBuilder().build() as client:
        req = client.get(httpbin_url() / "status/400").build()
        resp = await req.send()  # No error
        assert resp.status == 400
        print({"status": resp.status})


async def example_base_url() -> None:
    """Client base URL"""
    async with ClientBuilder().base_url(httpbin_url()).error_for_status(True).build() as client:
        resp = await client.get("/base64/Zm9vYmFy").build().send()
        print({"status": resp.status, "body": await resp.text()})


async def example_read_bytes() -> None:
    """Read bytes"""
    async with ClientBuilder().error_for_status(True).build() as client:
        resp = await client.get(httpbin_url() / "bytes/16").query({"seed": 0}).build().send()
        body = await resp.bytes()
        print({"status": resp.status, "body": body})


async def example_read_text() -> None:
    """Read text"""
    async with ClientBuilder().error_for_status(True).build() as client:
        resp = await client.get(httpbin_url() / "encoding/utf8").build().send()
        text = await resp.text()
        print({"status": resp.status, "text": text[:21]})


async def example_headers() -> None:
    """Headers usage"""
    async with (
        ClientBuilder()
        .default_headers({"X-Client": "client_value"})
        .user_agent("ua-example/1.0")  # Default is "python-pyreqwest/1.0.0"
        .error_for_status(True)
        .build() as client
    ):
        req = client.get(httpbin_url() / "headers").headers({"X-Req": "req_value"}).build()
        req.headers["X-Req2"] = "req2_value"  # Can also modify directly
        data = await (await req.send()).json()
        headers = data["headers"]
        print(
            {
                "X-Client": headers["X-Client"],
                "X-Req": headers["X-Req"],
                "X-Req2": headers["X-Req2"],
                "User-Agent": headers["User-Agent"],
            }
        )


if __name__ == "__main__":
    asyncio.run(run_examples(sys.modules[__name__]))
