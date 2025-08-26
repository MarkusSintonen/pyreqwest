from collections.abc import AsyncGenerator, Generator
from pathlib import Path

import pytest
import trustme

from .servers.echo_body_parts_server import EchoBodyPartsServer
from .servers.echo_server import EchoServer


@pytest.fixture(scope="session")
async def echo_server() -> AsyncGenerator[EchoServer]:
    async with EchoServer().serve_context() as server:
        assert str(server.url).startswith("http://")
        yield server


@pytest.fixture(scope="session")
async def echo_body_parts_server() -> AsyncGenerator[EchoBodyPartsServer]:
    async with EchoBodyPartsServer().serve_context() as server:
        assert str(server.url).startswith("http://")
        yield server


@pytest.fixture(scope="session")
def cert_authority() -> trustme.CA:
    return trustme.CA()


@pytest.fixture(scope="session")
def ca_pem_file(cert_authority: trustme.CA) -> Generator[Path, None, None]:
    with cert_authority.cert_pem.tempfile() as tmp:
        yield Path(tmp)


@pytest.fixture(scope="session")
def localhost_cert(cert_authority: trustme.CA) -> trustme.LeafCert:
    return cert_authority.issue_cert("127.0.0.1", "localhost")


@pytest.fixture(scope="session")
def cert_pem_file(localhost_cert: trustme.LeafCert) -> Generator[Path, None, None]:
    with localhost_cert.cert_chain_pems[0].tempfile() as tmp:
        yield Path(tmp)


@pytest.fixture(scope="session")
def cert_private_key_file(localhost_cert: trustme.LeafCert) -> Generator[Path, None, None]:
    with localhost_cert.private_key_pem.tempfile() as tmp:
        yield Path(tmp)


@pytest.fixture(scope="session")
async def https_echo_server(cert_pem_file: Path, cert_private_key_file: Path) -> AsyncGenerator[EchoServer]:
    async with EchoServer(ssl_key=cert_private_key_file, ssl_cert=cert_pem_file).serve_context() as server:
        assert str(server.url).startswith("https://")
        yield server


@pytest.fixture(scope="session")
async def https_echo_server_proxy(cert_pem_file: Path, cert_private_key_file: Path) -> AsyncGenerator[EchoServer]:
    async with EchoServer(ssl_key=cert_private_key_file, ssl_cert=cert_pem_file).serve_context() as server:
        assert str(server.url).startswith("https://")
        yield server
