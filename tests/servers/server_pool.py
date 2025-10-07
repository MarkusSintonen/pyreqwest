import asyncio
from collections import defaultdict
from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager
from typing import Self

from .server import ASGIApp, ServerConfig, find_free_port
from .server_subprocess import SubprocessServer


class ServerPool:
    def __init__(self) -> None:
        self._pools: dict[tuple[type[ASGIApp], str], asyncio.Queue[SubprocessServer]] = defaultdict(asyncio.Queue)
        self._ports: set[int] = set()

    @asynccontextmanager
    async def use_server(self, server_type: type[ASGIApp], config: ServerConfig) -> AsyncGenerator[SubprocessServer]:
        pool = self._pools[(server_type, config.dump_json())]

        server = await self._pop_server(pool, server_type, config)
        try:
            yield server
        finally:
            if server.running:
                await pool.put(server)
            else:
                await self._check_count(pool, server_type, config)

    async def _pop_server(
        self, pool: asyncio.Queue[SubprocessServer], server_type: type[ASGIApp], config: ServerConfig
    ) -> SubprocessServer:
        await self._check_count(pool, server_type, config)

        while True:
            server = await asyncio.wait_for(pool.get(), timeout=4.0)
            if server.running:
                return server
            await self._check_count(pool, server_type, config)

    async def _check_count(
        self, pool: asyncio.Queue[SubprocessServer], server_type: type[ASGIApp], config: ServerConfig
    ) -> None:
        if pool.qsize() < 2:
            for _ in range(2):
                await self._start_new(pool, server_type, config)

    async def _start_new(
        self,
        pool: asyncio.Queue[SubprocessServer],
        server_type: type[ASGIApp],
        config: ServerConfig,
        port: int | None = None,
    ) -> None:
        port = port or find_free_port(not_in_ports=self._ports)
        self._ports.add(port)
        await pool.put(await SubprocessServer.start(server_type, config, port))

    async def __aenter__(self) -> Self:
        return self

    async def __aexit__(self, *args: object) -> None:
        for server_queue in self._pools.values():
            while not server_queue.empty():
                server = await server_queue.get()
                await server.kill()
