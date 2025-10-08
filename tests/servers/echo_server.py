import asyncio
import gzip
import json
from collections.abc import Awaitable, Callable
from datetime import UTC, datetime
from typing import Any
from urllib.parse import parse_qsl

from .server import receive_all


class EchoServer:
    async def __call__(
        self,
        scope: dict[str, Any],
        receive: Callable[[], Awaitable[dict[str, Any]]],
        send: Callable[[dict[str, Any]], Awaitable[None]],
    ) -> None:
        assert scope["type"] == "http"

        query: list[tuple[str, str]] = [(k.decode(), v.decode()) for k, v in parse_qsl(scope["query_string"])]
        query_dict: dict[str, str] = dict(query)

        if sleep_start := float(query_dict.get("sleep_start", 0)):
            await asyncio.sleep(sleep_start)

        if query_dict.get("echo_only_body") == "1":
            resp_body = b"".join([b async for b in receive_all(receive)])
            resp_headers = [[b"content-type", b"application/octet-stream"]]
        else:
            resp = {
                "headers": scope["headers"],
                "http_version": scope["http_version"],
                "method": scope["method"],
                "path": scope["path"],
                "query": query,
                "raw_path": scope["raw_path"],
                "scheme": scope["scheme"],
                "body_parts": [b async for b in receive_all(receive)],
                "time": datetime.now(UTC).isoformat(),
            }
            resp_body = json_dump(resp)
            resp_headers = [[b"content-type", b"application/json"]]

        if query_dict.get("compress") in ("gzip", "gzip_invalid"):
            resp_body = gzip.compress(resp_body)
            if query_dict.get("compress") == "gzip_invalid":
                resp_body = resp_body[5:]
            resp_headers.extend([[b"content-encoding", b"gzip"], [b"x-content-encoding", b"gzip"]])

        for k, v in query:
            if k == "header_repeat":
                val, count = v.split(":", 1)
                resp_headers.append([b"X-Header-", val.encode() * int(count)])
            elif k.startswith("header_"):
                resp_headers.append([k.removeprefix("header_").replace("_", "-").encode(), v.encode()])

        await send(
            {
                "type": "http.response.start",
                "status": int(query_dict.get("status", 200)),
                "headers": resp_headers,
            },
        )

        if sleep_body := float(query_dict.get("sleep_body", 0)):
            part1, part2 = resp_body[: len(resp_body) // 2], resp_body[len(resp_body) // 2 :]
            await send({"type": "http.response.body", "body": part1, "more_body": True})
            await asyncio.sleep(sleep_body)
            await send({"type": "http.response.body", "body": part2})
        else:
            await send({"type": "http.response.body", "body": resp_body})


def json_dump(obj: Any) -> bytes:
    def default(val: Any) -> Any:
        if isinstance(val, bytes):
            return val.decode("utf-8")
        raise TypeError

    return json.dumps(obj, default=default).encode()
