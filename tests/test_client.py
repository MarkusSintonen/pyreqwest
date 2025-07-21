import asyncio
from datetime import timedelta

import pytest

from pyreqwest.client import ClientBuilder
from pyreqwest.exceptions import StatusError, PoolTimeoutError, ConnectTimeoutError, ReadTimeoutError
from pyreqwest.http import Url
from .servers.echo_server import EchoServer


@pytest.mark.parametrize("value", [True, False])
async def test_error_for_status(echo_server: EchoServer, value: bool):
    url = Url(str(echo_server.address))
    url.set_query_dict({"status": str(400)})

    async with ClientBuilder().error_for_status(value).build() as client:
        req = client.get(url).build_consumed()
        if value:
            with pytest.raises(StatusError) as e:
                await req.send()
            assert e.value.details["status"] == 400
        else:
            assert (await req.send()).status == 400


@pytest.mark.parametrize("value", [1, 2, None])
async def test_max_connections(echo_server: EchoServer, value: int | None):
    url = Url(str(echo_server.address))
    url.set_query_dict({"sleep_start": str(0.1)})

    builder = ClientBuilder().error_for_status(True).max_connections(value).pool_timeout(timedelta(seconds=0.05))

    async with builder.build() as client:
        async def request():
            await client.get(url).build_consumed().send()

        coro = asyncio.gather(request(), request())
        if value == 1:
            with pytest.raises(PoolTimeoutError) as e:
                await coro
            assert isinstance(e.value, TimeoutError)
        else:
            await coro


@pytest.mark.parametrize("value", [0.05, 0.2, None])
@pytest.mark.parametrize("sleep_kind", ["sleep_start", "sleep_body"])
async def test_timeout(echo_server: EchoServer, value: float | None, sleep_kind: str):
    url = Url(str(echo_server.address))
    url.set_query_dict({sleep_kind: str(0.1)})

    builder = ClientBuilder().error_for_status(True)
    if value is not None:
        builder = builder.timeout(timedelta(seconds=value))

    async with builder.build() as client:
        async def request():
            await client.get(url).build_consumed().send()

        coro = request()
        if value and value < 0.2:
            exc = ConnectTimeoutError if sleep_kind == "sleep_start" else ReadTimeoutError
            with pytest.raises(exc) as e:
                await coro
            assert isinstance(e.value, TimeoutError)
        else:
            await coro


@pytest.mark.parametrize("str_url", [False, True])
async def test_http_methods(echo_server: EchoServer, str_url: bool):
    url = str(echo_server.address) if str_url else echo_server.address
    async with ClientBuilder().error_for_status(True).build() as client:
        async with client.get(url).build_streamed() as response:
            assert (await response.json())['method'] == 'GET'
        async with client.post(url).build_streamed() as response:
            assert (await response.json())['method'] == 'POST'
        async with client.put(url).build_streamed() as response:
            assert (await response.json())['method'] == 'PUT'
        async with client.patch(url).build_streamed() as response:
            assert (await response.json())['method'] == 'PATCH'
        async with client.delete(url).build_streamed() as response:
            assert (await response.json())['method'] == 'DELETE'
        async with client.request("QUERY", url).build_streamed() as response:
            assert (await response.json())['method'] == 'QUERY'
