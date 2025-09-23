import asyncio
from collections import defaultdict
from collections.abc import AsyncGenerator, Generator
from contextlib import asynccontextmanager
from pathlib import Path
from tempfile import NamedTemporaryFile
from typing import Self

import pytest
import trustme

from .servers.echo_body_parts_server import EchoBodyPartsServer
from .servers.echo_server import EchoServer
from .servers.server import ASGIApp, ServerConfig, find_free_port
from .servers.server_subprocess import SubprocessServer


class ServerPool:
    def __init__(self) -> None:
        self._pools: dict[tuple[type[ASGIApp], str], asyncio.Queue[SubprocessServer]] = defaultdict(asyncio.Queue)
        self._ports: set[int] = set()

    @asynccontextmanager
    async def use_server(self, server_type: type[ASGIApp], config: ServerConfig) -> AsyncGenerator[SubprocessServer]:
        pool = self._pools[(server_type, config.model_dump_json())]

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
            server_queue.shutdown(immediate=True)

            while not server_queue.empty():
                server = await server_queue.get()
                await server.kill()


@pytest.fixture(scope="session")
async def server_pool() -> AsyncGenerator[ServerPool]:
    async with ServerPool() as pool:
        yield pool


@pytest.fixture
async def echo_server(server_pool: ServerPool) -> AsyncGenerator[SubprocessServer]:
    async with server_pool.use_server(EchoServer, ServerConfig()) as server:
        assert str(server.url).startswith("http://")
        yield server


@pytest.fixture
async def echo_body_parts_server(server_pool: ServerPool) -> AsyncGenerator[SubprocessServer]:
    async with server_pool.use_server(EchoBodyPartsServer, ServerConfig()) as server:
        assert str(server.url).startswith("http://")
        yield server


@pytest.fixture(scope="session")
def cert_authority() -> trustme.CA:
    return trustme.CA()


@pytest.fixture(scope="session")
def cert_authority_pem(cert_authority: trustme.CA) -> Generator[Path, None, None]:
    with NamedTemporaryFile(suffix=".pem") as tmp:
        tmp.write(cert_authority.cert_pem.bytes())
        tmp.flush()
        yield Path(tmp.name)


@pytest.fixture(scope="session")
def localhost_cert(cert_authority: trustme.CA) -> trustme.LeafCert:
    return cert_authority.issue_cert("127.0.0.1", "localhost")


@pytest.fixture(scope="session")
def cert_pem_file(localhost_cert: trustme.LeafCert) -> Generator[Path, None, None]:
    with NamedTemporaryFile(suffix=".pem") as tmp:
        tmp.write(localhost_cert.cert_chain_pems[0].bytes())
        tmp.flush()
        yield Path(tmp.name)


@pytest.fixture(scope="session")
def cert_private_key_file(localhost_cert: trustme.LeafCert) -> Generator[Path, None, None]:
    with NamedTemporaryFile(suffix=".pem") as tmp:
        tmp.write(localhost_cert.private_key_pem.bytes())
        tmp.flush()
        yield Path(tmp.name)


@pytest.fixture(scope="session")
async def https_echo_server(
    server_pool: ServerPool, cert_private_key_file: Path, cert_pem_file: Path, cert_authority_pem: Path
) -> AsyncGenerator[SubprocessServer]:
    config = ServerConfig(
        ssl_key=cert_private_key_file,
        ssl_cert=cert_pem_file,
        ssl_ca=cert_authority_pem,
    )
    async with server_pool.use_server(EchoServer, config) as server:
        assert str(server.url).startswith("https://")
        yield server
