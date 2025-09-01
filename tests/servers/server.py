import asyncio
import socket
from abc import ABC, abstractmethod
from collections.abc import AsyncIterable, Awaitable, Callable
from contextlib import asynccontextmanager, closing
from pathlib import Path
from typing import Any, Protocol

from granian.constants import HTTPModes, Interfaces
from granian.server.embed import Server as GranianServer
from pyreqwest.http import Url


class ASGIApp(Protocol):
    async def __call__(
        self,
        scope: dict[str, Any],
        receive: Callable[[], Awaitable[dict[str, Any]]],
        send: Callable[[dict[str, Any]], Awaitable[None]],
    ) -> None: ...


class Server(GranianServer, ABC):
    def __init__(
        self,
        ssl_cert: Path | None = None,
        ssl_key: Path | None = None,
        ssl_key_password: str | None = None,
        ssl_ca: Path | None = None,
        ssl_client_verify: bool = False,
        http: HTTPModes = HTTPModes.auto,
    ) -> None:
        self.proto = "https" if ssl_key else "http"
        super().__init__(
            self.app,
            port=find_free_port(),
            interface=Interfaces.ASGINL,
            ssl_cert=ssl_cert,
            ssl_key=ssl_key,
            ssl_key_password=ssl_key_password,
            ssl_ca=ssl_ca,
            ssl_client_verify=ssl_client_verify,
            http=http,
        )

    @abstractmethod
    async def app(
        self,
        scope: dict[str, Any],
        receive: Callable[[], Awaitable[dict[str, Any]]],
        send: Callable[[dict[str, Any]], Awaitable[None]],
    ) -> None: ...

    @property
    def url(self) -> Url:
        return Url(f"{self.proto}://{self.bind_addr}:{self.bind_port}")

    @asynccontextmanager
    async def serve_context(self):
        task = asyncio.create_task(self.serve())
        try:
            yield self
        finally:
            self.stop()  # type: ignore[no-untyped-call]
            await task


def find_free_port() -> int:
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("", 0))
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        return int(s.getsockname()[1])


async def receive_all(receive: Callable[[], Awaitable[dict[str, Any]]]) -> AsyncIterable[bytes]:
    more_body = True
    while more_body:
        async with asyncio.timeout(5.0):
            message = await receive()
        if message.get("body", None):
            yield message["body"]
        more_body = message.get("more_body", False)
