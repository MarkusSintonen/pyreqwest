import orjson
import pytest

from pyreqwest.client import ClientBuilder
from pyreqwest.http import Url


@pytest.mark.parametrize("str_url", [False, True])
async def test_get(echo_server: Url, str_url: bool):
    client = ClientBuilder().build()
    response = await client.get(str(echo_server) if str_url else echo_server).build().send()
    assert response.status == 200
    assert response.headers['content-type'] == 'application/json'
    body = orjson.loads(memoryview(await response.bytes()))
    assert body['method'] == 'GET'
    assert body['raw_path'] == '/'
