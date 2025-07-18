from typing import AsyncGenerator

import pytest

from .servers.echo_server import EchoServer


@pytest.fixture
async def echo_server() -> AsyncGenerator[EchoServer]:
    async with EchoServer().serve_context() as server:
        yield server
