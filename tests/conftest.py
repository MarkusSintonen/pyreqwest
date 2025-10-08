from collections.abc import AsyncGenerator, Generator
from pathlib import Path

import pytest
import trustme

from tests.utils import temp_file

from .servers.echo_body_parts_server import EchoBodyPartsServer
from .servers.echo_server import EchoServer
from .servers.server import ServerConfig
from .servers.server_pool import ServerPool
from .servers.server_subprocess import SubprocessServer


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
    with temp_file(cert_authority.cert_pem.bytes(), suffix=".pem") as tmp:
        yield tmp


@pytest.fixture(scope="session")
def localhost_cert(cert_authority: trustme.CA) -> trustme.LeafCert:
    return cert_authority.issue_cert("127.0.0.1", "localhost")


@pytest.fixture(scope="session")
def cert_pem_file(localhost_cert: trustme.LeafCert) -> Generator[Path, None, None]:
    with temp_file(localhost_cert.cert_chain_pems[0].bytes(), suffix=".pem") as tmp:
        yield tmp


@pytest.fixture(scope="session")
def cert_private_key_file(localhost_cert: trustme.LeafCert) -> Generator[Path, None, None]:
    with temp_file(localhost_cert.private_key_pem.bytes(), suffix=".pem") as tmp:
        yield tmp


@pytest.fixture
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
