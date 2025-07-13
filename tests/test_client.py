import orjson
from yarl import URL

from pyreqwest.client import ClientBuilder


async def test_get(echo_server: URL):
    client = ClientBuilder().build()
    response = await client.get(str(echo_server)).build().send()
    assert response.status_code == 200
    assert response.headers['content-type'] == 'application/json'
    body = await response.read()
    assert orjson.loads(memoryview(body)) == b'Hello, world!'
