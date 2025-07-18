import asyncio
import socket
from contextlib import asynccontextmanager, closing
from typing import Protocol, Any, Callable, Awaitable

from granian.constants import Interfaces
from granian.server.embed import Server as GranianServer

from pyreqwest.http import Url


class ASGIApp(Protocol):
    async def __call__(
        self,
        scope: dict[str, Any],
        receive: Callable[[], Awaitable[dict[str, Any]]],
        send: Callable[[dict[str, Any]], Awaitable[None]]
    ) -> None: ...


class Server(GranianServer):
    def __init__(self, app: ASGIApp):
        super().__init__(app, port=Server.find_free_port(), interface=Interfaces.ASGINL)

    @property
    def address(self) -> Url:
        return Url(f"http://{self.bind_addr}:{self.bind_port}")

    @asynccontextmanager
    async def serve_context(self):
        task = asyncio.create_task(self.serve())
        try:
            yield self
        finally:
            self.stop()
            await task

    @staticmethod
    def find_free_port() -> int:
        with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
            s.bind(('', 0))
            s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            return s.getsockname()[1]
