"""Streaming usage.

Run directly:
    uv run python -m examples.stream
"""

import asyncio
import sys
from collections.abc import AsyncIterator

from pyreqwest.client import ClientBuilder

from ._utils import httpbin_url, parse_data_uri, run_examples


async def example_stream_download() -> None:
    """Streaming download"""
    async with (
        ClientBuilder().error_for_status(True).build() as client,
        client.get(httpbin_url() / "stream-bytes/500").query({"seed": 0, "chunk_size": 100}).build_streamed() as resp,
    ):
        chunks: list[bytes] = []
        while (chunk := await resp.body_reader.read(100)) is not None:
            chunks.append(bytes(chunk))
    print({"chunks": len(chunks), "total_bytes": sum(len(c) for c in chunks)})


async def example_stream_upload() -> None:
    """Streaming upload"""

    async def byte_stream() -> AsyncIterator[bytes]:
        for i in range(5):
            yield f"part-{i}_".encode()

    async with ClientBuilder().error_for_status(True).build() as client:
        req = client.post(httpbin_url() / "post").body_stream(byte_stream()).build()
        resp = await req.send()
        data = await resp.json()
        assert parse_data_uri(data["data"]) == "part-0_part-1_part-2_part-3_part-4_"
        print({"status": resp.status, "data": data["data"]})


if __name__ == "__main__":
    asyncio.run(run_examples(sys.modules[__name__]))
