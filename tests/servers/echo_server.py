import urllib
from typing import Any, Callable, Awaitable, AsyncIterable

from orjson import orjson

from .server import Server


class EchoServer(Server):
    def __init__(self):
        super().__init__(self.app)

    async def app(
        self,
        scope: dict[str, Any],
        receive: Callable[[], Awaitable[dict[str, Any]]],
        send: Callable[[dict[str, Any]], Awaitable[None]],
    ) -> None:
        assert scope['type'] == 'http'

        resp = {
            "headers": scope['headers'],
            "http_version": scope['http_version'],
            "method": scope['method'],
            "path": scope['path'],
            "query": urllib.parse.parse_qsl(scope['query_string']),
            "raw_path": scope['raw_path'],
            "scheme": scope['scheme'],
            "body_parts": [b async for b in receive_all(receive)],
        }

        await send({
            'type': 'http.response.start',
            'status': 200,
            'headers': [[b'content-type', b'application/json']],
        })
        await send({'type': 'http.response.body', 'body': json_dump(resp)})


def json_dump(obj: Any) -> bytes:
    def default(val):
        if isinstance(val, bytes):
            return val.decode('utf-8')
        raise TypeError

    return orjson.dumps(obj, default=default)


async def receive_all(receive: Callable[[], Awaitable[dict[str, Any]]]) -> AsyncIterable[bytes]:
    more_body = True
    while more_body:
        message = await receive()
        if message.get('body', None):
            yield message['body']
        more_body = message.get('more_body', False)
