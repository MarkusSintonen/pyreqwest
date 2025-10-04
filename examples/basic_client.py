"""Basic usage examples for pyreqwest.

Run directly:
    uv run python -m examples.basic_client

Set HTTPBIN env var to point elsewhere if needed.
"""

import asyncio
import json
import os
import sys
from datetime import timedelta
from typing import Any

from pyreqwest.client import ClientBuilder
from pyreqwest.client.types import JsonDumpsContext
from pyreqwest.exceptions import ConnectTimeoutError
from pyreqwest.http import Url

HTTPBIN = Url(os.environ.get("HTTPBIN", "https://httpbin.org"))


async def example_simple_get() -> None:
    """Example 1: Simple GET"""
    async with ClientBuilder().error_for_status(True).build() as client:
        resp = await client.get(HTTPBIN / "get").query({"q": "pyreqwest"}).build().send()
        data = await resp.json()
        print(
            {
                "example": "simple_get",
                "status": resp.status,
                "args": data.get("args"),
                "url": data.get("url"),
            }
        )


async def example_post_json() -> None:
    """Example 2: POST JSON"""
    async with ClientBuilder().error_for_status(True).build() as client:
        payload: dict[str, str] = {"message": "hello"}
        resp = await client.post(HTTPBIN / "post").body_json(payload).build().send()
        data = await resp.json()
        print({"example": "post_json", "status": resp.status, "echo": data.get("json")})


async def example_stream_download() -> None:
    """Example 3: Streaming download"""
    async with (
        ClientBuilder().error_for_status(True).build() as client,
        client.get(HTTPBIN / "stream/5").build_streamed() as resp,
    ):
        chunks: list[bytes] = []
        while (chunk := await resp.body_reader.read_chunk()) is not None:
            chunks.append(bytes(chunk))
    print(
        {
            "example": "stream_download",
            "status": resp.status,
            "chunks": len(chunks),
            "total_bytes": sum(len(c) for c in chunks),
        }
    )


async def example_concurrent_requests() -> None:
    """Example 4: Concurrency"""
    async with ClientBuilder().error_for_status(True).build() as client:

        async def fetch(i: int) -> Any:
            r = await client.get(HTTPBIN / "get").query({"i": i}).build().send()
            return await r.json()

        results = await asyncio.gather(*(fetch(i) for i in range(3)))
        print(
            {
                "example": "concurrent_requests",
                "count": len(results),
                "indices": sorted(int(r["args"]["i"]) for r in results),
            }
        )


async def example_timeouts() -> None:
    """Example 5: Timeouts"""
    async with ClientBuilder().timeout(timedelta(seconds=1)).error_for_status(True).build() as client:
        req = client.get(HTTPBIN / "delay/2").build()
        try:
            await req.send()
            raise RuntimeError("should have raised")
        except ConnectTimeoutError as e:
            print({"example": "timeouts", "error": str(e)})


async def example_custom_json_dumps() -> None:
    """Example 6: Custom JSON dumps (sync)"""

    def dumps(ctx: JsonDumpsContext) -> bytes:
        data = ctx.data
        if isinstance(data, dict):
            return json.dumps({**data, "_trace": "demo"}).encode()
        return json.dumps(data).encode()

    async with ClientBuilder().json_handler(dumps=dumps).error_for_status(True).build() as client:
        resp = await client.post(HTTPBIN / "post").body_json({"value": 1}).build().send()
        data = await resp.json()
        print(
            {
                "example": "custom_json_dumps",
                "status": resp.status,
                "json_keys": sorted(data.get("json", {}).keys()),
            }
        )


async def example_session_features() -> None:
    """Example 7: Session (cookies, headers)"""
    async with (
        ClientBuilder()
        .default_cookie_store(True)
        .default_headers({"X-Client": "pyreqwest-demo"})
        .user_agent("pyreqwest-example/1.0")
        .error_for_status(True)
        .build() as client
    ):
        resp = await client.get(HTTPBIN / "headers").build().send()
        data = await resp.json()
        headers = data.get("headers", {})
        print(
            {
                "example": "session_features",
                "status": resp.status,
                "x_client": headers.get("X-Client"),
                "user_agent": headers.get("User-Agent"),
            }
        )


if __name__ == "__main__":  # pragma: no cover
    from ._utils import run_examples

    asyncio.run(run_examples(sys.modules[__name__]))
