from collections.abc import Generator
from datetime import timedelta
from typing import TypeVar

import pytest
from pyreqwest.client import BlockingClient, BlockingClientBuilder

from tests.servers.server import Server

T = TypeVar("T")


@pytest.fixture
def client() -> Generator[BlockingClient, None, None]:
    with BlockingClientBuilder().error_for_status(True).timeout(timedelta(seconds=5)).build() as client:
        yield client


def test_blocking_send(client: BlockingClient, echo_server: Server) -> None:
    req = client.get(echo_server.url).build_consumed()
    resp = req.send()
    assert resp.headers["content-type"] == "application/json"
