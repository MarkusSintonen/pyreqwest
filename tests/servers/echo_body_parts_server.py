import asyncio
from json import JSONDecodeError
from typing import Any, Callable, Awaitable

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

        await send({
            'type': 'http.response.start',
            'status': 200,
            'headers': [[b'content-type', b'application/json']],
        })

        chunks = [chunk async for chunk in receive_all(receive) if chunk]

        for chunk in chunks:
            if sleep := (try_json(chunk) or {}).get('sleep'):
                await asyncio.sleep(sleep)
            await send({'type': 'http.response.body', 'body': chunk, 'more_body': True})
        await send({'type': 'http.response.body', 'body': b"", 'more_body': False})


def try_json(data: bytes) -> dict[str, Any] | None:
    try:
        return orjson.loads(data)
    except JSONDecodeError:
        return None
