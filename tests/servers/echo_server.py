import asyncio
import gzip
from typing import Any, Callable, Awaitable, AsyncIterable
from urllib.parse import parse_qsl

from orjson import orjson

from .server import Server, receive_all


class EchoServer(Server):
    async def app(
        self,
        scope: dict[str, Any],
        receive: Callable[[], Awaitable[dict[str, Any]]],
        send: Callable[[dict[str, Any]], Awaitable[None]],
    ) -> None:
        assert scope['type'] == 'http'
        query = [(k.decode(), v.decode()) for k, v in parse_qsl(scope['query_string'])]
        query_dict = dict(query)

        if sleep_start := float(query_dict.get('sleep_start', 0)):
            await asyncio.sleep(sleep_start)

        resp = {
            "headers": scope['headers'],
            "http_version": scope['http_version'],
            "method": scope['method'],
            "path": scope['path'],
            "query": query,
            "raw_path": scope['raw_path'],
            "scheme": scope['scheme'],
            "body_parts": [b async for b in receive_all(receive)],
        }
        resp_headers = [[b'content-type', b'application/json']]

        resp_body = json_dump(resp)
        if query_dict.get('compress') == "gzip":
            resp_body = gzip.compress(resp_body)
            resp_headers.extend([[b'content-encoding', b'gzip'], [b'x-content-encoding', b'gzip']])

        await send({
            'type': 'http.response.start',
            'status': int(query_dict.get('status', 200)),
            'headers': resp_headers,
        })

        if sleep_body := float(query_dict.get('sleep_body', 0)):
            part1, part2 = resp_body[:len(resp_body) // 2], resp_body[len(resp_body) // 2:]
            await send({'type': 'http.response.body', 'body': part1, 'more_body': True})
            await asyncio.sleep(sleep_body)
            await send({'type': 'http.response.body', 'body': part2})
        else:
            await send({'type': 'http.response.body', 'body': resp_body})


def json_dump(obj: Any) -> bytes:
    def default(val):
        if isinstance(val, bytes):
            return val.decode('utf-8')
        raise TypeError

    return orjson.dumps(obj, default=default)
