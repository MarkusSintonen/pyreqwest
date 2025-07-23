from typing import Any, Callable, Awaitable

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

        async for chunk in receive_all(receive):
            if chunk:
                await send({'type': 'http.response.body', 'body': chunk, 'more_body': True})
        await send({'type': 'http.response.body', 'body': b"", 'more_body': False})
