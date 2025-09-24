import asyncio
import queue
import random
import socket
import time
from asyncio import AbstractEventLoop
from collections.abc import AsyncGenerator, AsyncIterable, Awaitable, Callable
from contextlib import asynccontextmanager, closing, suppress
from datetime import timedelta
from functools import cached_property
from pathlib import Path
from threading import Thread
from typing import Any, Protocol, Self

from granian.constants import HTTPModes, Interfaces
from granian.server.embed import Server as GranianServer
from pydantic import BaseModel
from pyreqwest.client import ClientBuilder
from pyreqwest.http import Url


class ServerConfig(BaseModel):
    ssl_cert: Path | None = None
    ssl_key: Path | None = None
    ssl_ca: Path | None = None
    http: HTTPModes = HTTPModes.auto

    @property
    def is_https(self) -> bool:
        return bool(self.ssl_key)

    @cached_property
    def ca_pem_bytes(self) -> bytes | None:
        return self.ssl_ca.read_bytes() if self.ssl_ca else None


class ASGIApp(Protocol):
    async def __call__(
        self,
        scope: dict[str, Any],
        receive: Callable[[], Awaitable[dict[str, Any]]],
        send: Callable[[dict[str, Any]], Awaitable[None]],
    ) -> None: ...


class EmbeddedServer(GranianServer):
    def __init__(self, app: ASGIApp, port: int, config: ServerConfig) -> None:
        self.config = config
        super().__init__(
            app,
            port=port,
            interface=Interfaces.ASGINL,
            ssl_cert=config.ssl_cert,
            ssl_key=config.ssl_key,
            http=config.http,
        )

    @property
    def url(self) -> Url:
        proto = "https" if self.ssl_ctx[0] else "http"
        return Url(f"{proto}://{self.bind_addr}:{self.bind_port}")

    @asynccontextmanager
    async def serve_context(self) -> AsyncGenerator[Self]:
        server_loop_chan: queue.Queue[AbstractEventLoop] = queue.Queue(maxsize=1)

        def server_runner() -> None:
            with asyncio.Runner() as runner:
                server_loop_chan.put_nowait(runner.get_loop())
                runner.run(self.serve())

        server_thread = Thread(target=server_runner, daemon=True)
        server_thread.start()

        try:
            await wait_for_server(self.url, ca_pem=self.config.ca_pem_bytes)
            yield self
        finally:
            server_loop_chan.get(timeout=5).call_soon_threadsafe(self.stop)
            server_thread.join(timeout=5)
            assert not server_thread.is_alive()


def is_port_free(port: int) -> bool:
    with suppress(OSError), closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("", port))
        return True
    return False


def find_free_port(*, not_in_ports: set[int] | None = None, timeout: timedelta = timedelta(seconds=5)) -> int:
    not_in_ports = not_in_ports or set()
    deadline = time.monotonic() + timeout.total_seconds()
    while True:
        port = random.randint(49152, 60999)
        if port not in not_in_ports and is_port_free(port):
            return port
        if time.monotonic() > deadline:
            raise TimeoutError("Could not find a free port")


async def receive_all(receive: Callable[[], Awaitable[dict[str, Any]]]) -> AsyncIterable[bytes]:
    more_body = True
    while more_body:
        async with asyncio.timeout(5.0):
            message = await receive()
        if part := message.get("body"):
            yield part
        more_body = message.get("more_body", False)


async def wait_for_server(url: Url, ca_pem: bytes | None, success_timeout: timedelta = timedelta(seconds=5)) -> None:
    deadline = time.monotonic() + success_timeout.total_seconds()
    if url.scheme == "https":
        assert ca_pem

    builder = ClientBuilder().error_for_status(True).timeout(timedelta(seconds=1))
    if ca_pem:
        builder = builder.add_root_certificate_pem(ca_pem)

    async with builder.build() as client:
        while True:
            try:
                await client.get(url).build().send()
                return
            except Exception as exc:
                if time.monotonic() > deadline:
                    print(exc)
                    raise
                await asyncio.sleep(0.1)
