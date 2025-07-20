import pytest

from pyreqwest.client import ClientBuilder
from .servers.echo_server import EchoServer


@pytest.mark.parametrize("str_url", [False, True])
async def test_http_methods(echo_server: EchoServer, str_url: bool):
    url = str(echo_server.address) if str_url else echo_server.address
    async with ClientBuilder().error_for_status(True).build() as client:
        async with client.get(url).build() as response:
            assert (await response.json())['method'] == 'GET'
        async with client.post(url).build() as response:
            assert (await response.json())['method'] == 'POST'
        async with client.put(url).build() as response:
            assert (await response.json())['method'] == 'PUT'
        async with client.patch(url).build() as response:
            assert (await response.json())['method'] == 'PATCH'
        async with client.delete(url).build() as response:
            assert (await response.json())['method'] == 'DELETE'
        async with client.request("QUERY", url).build() as response:
            assert (await response.json())['method'] == 'QUERY'
