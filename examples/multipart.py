"""Multipart usage.

Run directly:
    uv run python -m examples.multipart
"""

import asyncio
import sys
from collections.abc import AsyncIterator
from pathlib import Path
from tempfile import NamedTemporaryFile

from pyreqwest.client import ClientBuilder
from pyreqwest.multipart import FormBuilder, PartBuilder

from ._utils import httpbin_url, run_examples


async def example_multipart_text() -> None:
    """Multipart text fields"""
    form = FormBuilder().text("name", "John").text("email", "john@example.com")
    async with ClientBuilder().error_for_status(True).build() as client:
        resp = await client.post(httpbin_url() / "post").multipart(form).build().send()
        form = (await resp.json())["form"]
        print({"form": {"name": form["name"], "email": form["email"]}})


async def example_multipart_file_upload() -> None:
    """Multipart file upload (custom part)"""
    with NamedTemporaryFile() as fp:
        fp.write(b"file-content")
        fp.flush()
        path = Path(fp.name)
        part = (await PartBuilder.from_file(path)).mime("text/plain").file_name("demo.txt")  # Filename for temp file

    form = FormBuilder().text("description", "demo").part("my_file", part)
    async with ClientBuilder().error_for_status(True).build() as client:
        resp = await client.post(httpbin_url() / "post").multipart(form).build().send()
        data = await resp.json()
        print({"form": {"description": data["form"]["description"]}, "file": data["files"]["my_file"]})


async def example_multipart_streaming_part() -> None:
    """Multipart streaming part"""

    async def stream() -> AsyncIterator[bytes]:
        yield b"streamed-"
        yield b"file-"
        yield b"content"

    part = PartBuilder.from_stream(stream()).mime("text/plain").file_name("streamed.txt")
    form = FormBuilder().text("description", "streamed").part("my_file", part)
    async with ClientBuilder().error_for_status(True).build() as client:
        resp = await client.post(httpbin_url() / "post").multipart(form).build().send()
        data = await resp.json()
        print({"form": {"description": data["form"]["description"]}, "file": data["files"]["my_file"]})


if __name__ == "__main__":
    asyncio.run(run_examples(sys.modules[__name__]))
