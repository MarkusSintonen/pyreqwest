from collections.abc import AsyncGenerator, Generator
from datetime import timedelta

import httpx
import pytest
from pyreqwest.client import ClientBuilder, SyncClientBuilder
from pyreqwest.compatibility.httpx import HttpxTransport, SyncHttpxTransport
from pyreqwest.exceptions import ClientClosedError, StatusError
from pyreqwest.http import HeaderMap
from pyreqwest.response import Response, SyncResponse

from tests.utils import IS_CI

from .servers.server import find_free_port
from .servers.server_subprocess import SubprocessServer


@pytest.fixture
async def httpx_client() -> AsyncGenerator[httpx.AsyncClient, None]:
    async with httpx.AsyncClient(transport=HttpxTransport()) as client:
        yield client


@pytest.fixture
def sync_httpx_client() -> Generator[httpx.Client, None, None]:
    with httpx.Client(transport=SyncHttpxTransport()) as client:
        yield client


async def test_get(echo_server: SubprocessServer, httpx_client: httpx.AsyncClient):
    resp = await httpx_client.get(str(echo_server.url))
    assert resp.status_code == 200
    data = resp.json()
    assert data["method"] == "GET" and data["path"] == "/"


async def test_post_body(echo_server: SubprocessServer, httpx_client: httpx.AsyncClient):
    resp = await httpx_client.post(str(echo_server.url), json={"foo": "bar"})
    assert resp.status_code == 200
    data = resp.json()
    assert data["method"] == "POST" and data["body_parts"] == ['{"foo":"bar"}']


@pytest.mark.parametrize("override", [{}, {"X-Default": "override"}, {"x-default": "override2"}])
async def test_headers(echo_server: SubprocessServer, override: dict[str, str]):
    async with (
        ClientBuilder().default_headers({"X-Default": "foo"}).build() as client,
        httpx.AsyncClient(transport=HttpxTransport(client)) as httpx_client,
    ):
        resp = await httpx_client.get(str(echo_server.url), headers={"X-Test": "bar", **override})
        assert resp.status_code == 200
        headers = dict(resp.json()["headers"])
        assert headers["x-test"] == "bar"
        assert headers["x-default"] == HeaderMap(override).get("X-Default", "foo")
        assert headers["user-agent"] == "python-httpx/0.28.1"


async def test_body_stream(echo_server: SubprocessServer, httpx_client: httpx.AsyncClient):
    async def body_stream() -> AsyncGenerator[bytes, None]:
        for chunk in [b"hello ", b"world"]:
            yield chunk

    resp = await httpx_client.post(str(echo_server.url), content=body_stream())
    assert resp.json()["body_parts"] == ["hello ", "world"] and resp.status_code == 200


async def test_extensions(echo_server: SubprocessServer, httpx_client: httpx.AsyncClient):
    resp = await httpx_client.get(str(echo_server.url), extensions={"foo": "bar"})
    assert resp.status_code == 200
    assert "foo" not in resp.extensions
    inner_resp = resp.extensions["pyreqwest_response"]
    assert isinstance(inner_resp, Response)
    assert inner_resp.extensions["foo"] == "bar"


async def test_aclose(echo_server: SubprocessServer):
    transport = HttpxTransport()
    async with httpx.AsyncClient(transport=transport) as client:
        await transport.aclose()
        with pytest.raises(ClientClosedError):
            await client.get(str(echo_server.url))


async def test_context(echo_server: SubprocessServer):
    transport = HttpxTransport()

    async with httpx.AsyncClient(transport=transport) as client:
        resp = await client.get(str(echo_server.url))
        assert resp.status_code == 200

    # Now closed
    async with httpx.AsyncClient(transport=transport) as client:
        with pytest.raises(ClientClosedError):
            await client.get(str(echo_server.url))


async def test_timeout(echo_server: SubprocessServer):
    async with (
        ClientBuilder().timeout(timedelta(seconds=3.0)).build() as client,
        httpx.AsyncClient(
            transport=HttpxTransport(client),
            timeout=httpx.Timeout(timeout=0.1),  # Respects httpx req timeout
        ) as httpx_client,
    ):
        with pytest.raises(httpx.ConnectTimeout):
            await httpx_client.get(str(echo_server.url.with_query({"sleep_start": 2.0})))


@pytest.mark.parametrize("timeout", [None, httpx.Timeout(None), httpx.Timeout(5.0, connect=None)])
async def test_timeout_with_disabled_phase(echo_server: SubprocessServer, timeout: httpx.Timeout | None):
    async with (
        ClientBuilder().build() as client,
        httpx.AsyncClient(transport=HttpxTransport(client), timeout=timeout) as httpx_client,
    ):
        resp = await httpx_client.get(str(echo_server.url))
        assert resp.status_code == 200


async def test_connect_error(httpx_client: httpx.AsyncClient):
    with pytest.raises(httpx.ConnectError):
        await httpx_client.get(f"http://localhost:{find_free_port()}")


async def test_read_timeout_body(echo_server: SubprocessServer):
    timeout = 1.0 if IS_CI else 0.1
    async with (
        ClientBuilder().read_timeout(timedelta(seconds=timeout)).build() as client,
        httpx.AsyncClient(transport=HttpxTransport(client)) as httpx_client,
    ):
        with pytest.raises(httpx.ReadTimeout):
            await httpx_client.get(str(echo_server.url.with_query({"sleep_body": 2.0})))


@pytest.mark.parametrize("client_error_for_status", [True, False])
async def test_status_error(echo_server: SubprocessServer, client_error_for_status: bool):
    async with (
        ClientBuilder().error_for_status(client_error_for_status).build() as client,
        httpx.AsyncClient(transport=HttpxTransport(client)) as httpx_client,
    ):
        if client_error_for_status:
            # Uses pyreqwest StatusError as httpx only raises status error on response.raise_for_status()
            with pytest.raises(StatusError):
                await httpx_client.get(str(echo_server.url.with_query({"status": 404})))
        else:
            resp = await httpx_client.get(str(echo_server.url.with_query({"status": 404})))
            with pytest.raises(httpx.HTTPStatusError):
                resp.raise_for_status()


def test_sync_get(echo_server: SubprocessServer, sync_httpx_client: httpx.Client):
    resp = sync_httpx_client.get(str(echo_server.url))
    assert resp.status_code == 200
    data = resp.json()
    assert data["method"] == "GET" and data["path"] == "/"


def test_sync_post_body(echo_server: SubprocessServer, sync_httpx_client: httpx.Client):
    resp = sync_httpx_client.post(str(echo_server.url), json={"foo": "bar"})
    assert resp.status_code == 200
    data = resp.json()
    assert data["method"] == "POST" and data["body_parts"] == ['{"foo":"bar"}']


@pytest.mark.parametrize("override", [{}, {"X-Default": "override"}, {"x-default": "override2"}])
def test_sync_headers(echo_server: SubprocessServer, override: dict[str, str]):
    with (
        SyncClientBuilder().default_headers({"X-Default": "foo"}).build() as client,
        httpx.Client(transport=SyncHttpxTransport(client)) as httpx_client,
    ):
        resp = httpx_client.get(str(echo_server.url), headers={"X-Test": "bar", **override})
        assert resp.status_code == 200
        headers = dict(resp.json()["headers"])
        assert headers["x-test"] == "bar"
        assert headers["x-default"] == HeaderMap(override).get("X-Default", "foo")
        assert headers["user-agent"] == "python-httpx/0.28.1"


def test_sync_body_stream(echo_server: SubprocessServer, sync_httpx_client: httpx.Client):
    def body_stream() -> Generator[bytes, None, None]:
        yield from [b"hello ", b"world"]

    resp = sync_httpx_client.post(str(echo_server.url), content=body_stream())
    assert resp.json()["body_parts"] == ["hello ", "world"] and resp.status_code == 200


def test_sync_extensions(echo_server: SubprocessServer, sync_httpx_client: httpx.Client):
    resp = sync_httpx_client.get(str(echo_server.url), extensions={"foo": "bar"})
    assert resp.status_code == 200
    assert "foo" not in resp.extensions
    inner_resp = resp.extensions["pyreqwest_response"]
    assert isinstance(inner_resp, SyncResponse)
    assert inner_resp.extensions["foo"] == "bar"


def test_sync_close(echo_server: SubprocessServer):
    transport = SyncHttpxTransport()
    with httpx.Client(transport=transport) as client:
        transport.close()
        with pytest.raises(ClientClosedError):
            client.get(str(echo_server.url))


def test_sync_context(echo_server: SubprocessServer):
    transport = SyncHttpxTransport()

    with httpx.Client(transport=transport) as client:
        resp = client.get(str(echo_server.url))
        assert resp.status_code == 200

    # Now closed
    with httpx.Client(transport=transport) as client, pytest.raises(ClientClosedError):
        client.get(str(echo_server.url))


def test_sync_timeout(echo_server: SubprocessServer):
    with (
        SyncClientBuilder().timeout(timedelta(seconds=3.0)).build() as client,
        httpx.Client(
            transport=SyncHttpxTransport(client),
            timeout=httpx.Timeout(timeout=0.1),  # Respects httpx req timeout
        ) as httpx_client,
        pytest.raises(httpx.ConnectTimeout),
    ):
        httpx_client.get(str(echo_server.url.with_query({"sleep_start": 2.0})))


def test_sync_connect_error(sync_httpx_client: httpx.Client):
    with pytest.raises(httpx.ConnectError):
        sync_httpx_client.get(f"http://localhost:{find_free_port()}")


def test_sync_read_timeout_body(echo_server: SubprocessServer):
    timeout = 1.0 if IS_CI else 0.1
    with (
        SyncClientBuilder().read_timeout(timedelta(seconds=timeout)).build() as client,
        httpx.Client(transport=SyncHttpxTransport(client)) as httpx_client,
        pytest.raises(httpx.ReadTimeout),
    ):
        httpx_client.get(str(echo_server.url.with_query({"sleep_body": 2.0})))


@pytest.mark.parametrize("client_error_for_status", [True, False])
def test_sync_status_error(echo_server: SubprocessServer, client_error_for_status: bool):
    with (
        SyncClientBuilder().error_for_status(client_error_for_status).build() as client,
        httpx.Client(transport=SyncHttpxTransport(client)) as httpx_client,
    ):
        if client_error_for_status:
            # Uses pyreqwest StatusError as httpx only raises status error on response.raise_for_status()
            with pytest.raises(StatusError):
                httpx_client.get(str(echo_server.url.with_query({"status": 404})))
        else:
            resp = httpx_client.get(str(echo_server.url.with_query({"status": 404})))
            with pytest.raises(httpx.HTTPStatusError):
                resp.raise_for_status()
