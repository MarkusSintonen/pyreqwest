import asyncio
from json import JSONDecodeError
from typing import Any, Callable, Awaitable
from urllib.parse import parse_qsl

from orjson import orjson

from .server import Server, receive_all


class EchoBodyPartsServer(Server):
    async def app(
        self,
        scope: dict[str, Any],
        receive: Callable[[], Awaitable[dict[str, Any]]],
        send: Callable[[dict[str, Any]], Awaitable[None]],
    ) -> None:
        assert scope['type'] == 'http'
        query: dict[str, str] = dict((k.decode(), v.decode()) for k, v in parse_qsl(scope['query_string']))

        resp_headers = []
        if content_type := query.get('content_type'):
            resp_headers.append([b'content-type', content_type.encode()])
        else:
            resp_headers.append([b'content-type', b'application/json'])

        await send({
            'type': 'http.response.start',
            'status': 200,
            'headers': resp_headers,
        })

        chunks = [chunk async for chunk in receive_all(receive) if chunk]

        for chunk in chunks:
            if sleep := (try_json(chunk) or {}).get('sleep'):
                await asyncio.sleep(sleep)
            await send({'type': 'http.response.body', 'body': chunk, 'more_body': True})
        await send({'type': 'http.response.body', 'body': b"", 'more_body': False})


def try_json(data: bytes) -> dict[str, Any] | None:
    try:
        val = orjson.loads(data)
        return val if isinstance(val, dict) else None
    except JSONDecodeError:
        return None
