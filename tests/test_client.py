import orjson

from pyreqwest.client import ClientBuilder
from pyreqwest.http import Url


async def test_get(echo_server: Url):
    client = ClientBuilder().build()
    response = await client.get(echo_server).build().send()
    assert response.status_code == 200
    assert response.headers['content-type'] == 'application/json'
    body = orjson.loads(memoryview(await response.read()))
    assert body['method'] == 'GET'
    assert body['raw_path'] == '/'
