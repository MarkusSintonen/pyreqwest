"""JSON usage.

Run directly:
    uv run python -m examples.json_usage
"""

import asyncio
import base64
import json
import sys
from typing import Any

from pyreqwest.client import ClientBuilder
from pyreqwest.client.types import JsonDumpsContext, JsonLoadsContext

from ._utils import httpbin_url, run_examples


async def example_post_json() -> None:
    """POST JSON"""
    async with ClientBuilder().error_for_status(True).build() as client:
        resp = await client.post(httpbin_url() / "post").body_json({"message": "hello"}).build().send()
        data = await resp.json()
        assert data["headers"]["Content-Type"] == ["application/json"]  # Set by default
        print({"status": resp.status, "echo": data["json"], "content_type": data["headers"]["Content-Type"]})


async def example_custom_json_dumps() -> None:
    """Custom JSON dumps (bytes base64 serializer)"""

    def dumps(ctx: JsonDumpsContext) -> bytes:
        def ser_bytes(o: Any) -> str:
            if isinstance(o, bytes):
                return base64.b64encode(o).decode()
            raise TypeError

        return json.dumps(ctx.data, default=ser_bytes).encode()

    async with ClientBuilder().json_handler(dumps=dumps).error_for_status(True).build() as client:
        resp = await client.post(httpbin_url() / "post").body_json({"value": b"foo"}).build().send()
        data = await resp.json()
        assert data["json"] == {"value": "Zm9v"}
        print({"status": resp.status, "json": data["json"]})


async def example_custom_json_loads() -> None:
    """Custom JSON loads (bytes base64 deserializer)"""

    async def loads(ctx: JsonLoadsContext) -> Any:
        def des_bytes(d: dict[str, Any]) -> dict[str, Any]:
            if "value" in d:
                d["value"] = base64.b64decode(d["value"])
            return d

        return json.loads(bytes(await ctx.body_reader.bytes()), object_hook=des_bytes)

    async with ClientBuilder().json_handler(loads=loads).error_for_status(True).build() as client:
        resp = await client.post(httpbin_url() / "post").body_json({"value": "Zm9v"}).build().send()
        data = await resp.json()
        assert data["json"] == {"value": b"foo"}
        print({"status": resp.status, "json": data["json"]})


if __name__ == "__main__":
    asyncio.run(run_examples(sys.modules[__name__]))
