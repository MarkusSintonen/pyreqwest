from collections.abc import Generator
from datetime import timedelta
from typing import TypeVar

import pytest
from pyreqwest.client import Client, ClientBuilder

from tests.servers.server import Server

T = TypeVar("T")


@pytest.fixture
def client() -> Generator[Client, None, None]:
    with ClientBuilder().error_for_status(True).timeout(timedelta(seconds=5)).build() as client:
        yield client


def test_blocking_send(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).build_consumed()
    resp = req.blocking_send()
    assert resp.headers["content-type"] == "application/json"
