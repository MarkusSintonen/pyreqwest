from typing import AsyncGenerator

import pytest
import trustme

from pyreqwest.client import Client, ClientBuilder
from .servers.server import Server


@pytest.fixture
async def client(cert_authority: trustme.CA) -> AsyncGenerator[Client, None]:
    cert_pem = cert_authority.cert_pem.bytes()
    async with ClientBuilder().error_for_status(True).add_root_certificate_pem(cert_pem).build() as client:
        yield client


async def test_method(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).build_consumed()
    assert req.method == "GET"
    req.method = "POST"
    resp = await req.send()
    assert (await resp.json())['method'] == 'POST'


async def test_url(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).query({"a": "b"}).build_consumed()
    assert req.url == echo_server.url.with_query({"a": "b"})
    req.url = req.url.with_query({"test": "value"})
    assert req.url.query == {"test": "value"}


async def test_headers(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).headers({"X-Test1": "Value1", "X-Test2": "Value2"}).build_consumed()
    assert req.headers["X-Test1"] == "Value1" and req.headers["x-test1"] == "Value1"
    assert req.headers["X-Test2"] == "Value2" and req.headers["x-test2"] == "Value2"

    req.headers["X-Test3"] = "Value3"
    assert req.headers["X-Test3"] == "Value3" and req.headers["x-test3"] == "Value3"

    assert req.headers.pop("x-test1")
    assert "X-Test1" not in req.headers and "x-test1" not in req.headers

    resp = await req.send()
    assert [(k, v) for k, v in (await resp.json())['headers'] if k.startswith("x-")] == [
        ('x-test2', 'Value2'), ('x-test3', 'Value3')
    ]


async def test_extensions(client: Client, echo_server: Server) -> None:
    req = client.get(echo_server.url).extensions({"a": "b"}).build_consumed()
    assert req.extensions == {"a": "b"}
    req.extensions = {"foo": "bar", "test": "value"}
    assert req.extensions.pop("test") == "value"
    assert req.extensions == {"foo": "bar"}
