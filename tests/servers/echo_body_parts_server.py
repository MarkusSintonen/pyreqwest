import asyncio
import json
from collections.abc import Awaitable, Callable
from json import JSONDecodeError
from typing import Any
from urllib.parse import parse_qsl

from .server import receive_all


class EchoBodyPartsServer:
    async def __call__(
        self,
        scope: dict[str, Any],
        receive: Callable[[], Awaitable[dict[str, Any]]],
        send: Callable[[dict[str, Any]], Awaitable[None]],
    ) -> None:
        assert scope["type"] == "http"
        query: dict[str, str] = {k.decode(): v.decode() for k, v in parse_qsl(scope["query_string"])}

        resp_headers = []
        if content_type := query.get("content_type"):
            resp_headers.append([b"content-type", content_type.encode()])
        else:
            resp_headers.append([b"content-type", b"application/json"])

        await send(
            {
                "type": "http.response.start",
                "status": 200,
                "headers": resp_headers,
            },
        )

        async for chunk in receive_all(receive):
            if sleep := (try_json(chunk) or {}).get("sleep"):
                await asyncio.sleep(sleep)
            await send({"type": "http.response.body", "body": chunk, "more_body": True})
        await send({"type": "http.response.body", "body": b"", "more_body": False})


def try_json(data: bytes) -> dict[str, Any] | None:
    try:
        val = json.loads(data)
        return val if isinstance(val, dict) else None
    except (JSONDecodeError, UnicodeDecodeError):
        return None
