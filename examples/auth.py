"""Auth usage.

Run directly:
    uv run python -m examples.auth
"""

import asyncio
import sys

from pyreqwest.client import ClientBuilder
from pyreqwest.exceptions import StatusError

from ._utils import httpbin_url, run_examples


async def example_basic_auth() -> None:
    """Basic auth example"""
    async with ClientBuilder().error_for_status(True).build() as client:
        resp = await client.get(httpbin_url() / "basic-auth/user/passwd").basic_auth("user", "passwd").build().send()
        data = await resp.json()
        print({"authorized": data["authorized"], "user": data["user"]})

    # Wrong credentials
    async with ClientBuilder().error_for_status(True).build() as client:
        req = client.get(httpbin_url() / "basic-auth/user/passwd").basic_auth("user", "wrong").build()
        try:
            await req.send()
            raise RuntimeError("should have raised")
        except StatusError as e:
            assert e.details["status"] == 401
            print({"error": str(e)})


async def example_bearer_auth() -> None:
    """Bearer auth example"""
    async with ClientBuilder().error_for_status(True).build() as client:
        resp = await client.get(httpbin_url() / "bearer").bearer_auth("mytoken").build().send()
        data = await resp.json()
        print({"authenticated": data["authenticated"], "token": data["token"]})

    # No token
    async with ClientBuilder().error_for_status(True).build() as client:
        req = client.get(httpbin_url() / "bearer").bearer_auth("").build()  # Not set
        try:
            await req.send()
            raise RuntimeError("should have raised")
        except StatusError as e:
            assert e.details["status"] == 401
            print({"error": str(e)})


if __name__ == "__main__":
    asyncio.run(run_examples(sys.modules[__name__]))
