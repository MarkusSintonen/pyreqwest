import asyncio
import importlib
import sys

from pyreqwest.http import Url

from .server import ASGIApp, EmbeddedServer, ServerConfig, wait_for_server


class SubprocessServer:
    def __init__(
        self, server_type: type[ASGIApp], config: ServerConfig, port: int, process: asyncio.subprocess.Process
    ) -> None:
        self.server_type = server_type
        self.config = config
        self.port = port
        self._process: asyncio.subprocess.Process | None = process

    @staticmethod
    async def start(server_type: type[ASGIApp], config: ServerConfig, port: int) -> "SubprocessServer":
        process = await asyncio.create_subprocess_exec(
            sys.executable,
            "-m",
            "tests.servers.server_subprocess",
            f"{server_type.__module__}.{server_type.__name__}",
            str(port),
            config.dump_json(),
        )
        server = SubprocessServer(server_type, config, port, process)
        await wait_for_server(server.url, ca_pem=config.ca_pem_bytes)
        assert server.running
        return server

    @property
    def url(self) -> Url:
        proto = "https" if self.config.is_https else "http"
        return Url(f"{proto}://localhost:{self.port}")

    @property
    def running(self) -> bool:
        return self._process is not None and self._process.returncode is None

    async def kill(self) -> None:
        assert self._process
        self._process.kill()
        await self._process.wait()
        self._process = None

    async def restart(self) -> None:
        if self.running:
            await self.kill()
        restarted = await SubprocessServer.start(self.server_type, self.config, self.port)
        self._process = restarted._process


if __name__ == "__main__":
    module_path, class_name = sys.argv[1].rsplit(".", 1)
    asgi_class = getattr(importlib.import_module(module_path), class_name)

    port = int(sys.argv[2])
    config = ServerConfig.parse_json(sys.argv[3])

    asyncio.run(EmbeddedServer(asgi_class(), port, config).serve())
