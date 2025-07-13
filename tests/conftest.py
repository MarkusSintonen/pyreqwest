import asyncio
import json
import socket
from contextlib import closing
from typing import AsyncGenerator

import pytest
from granian.constants import Interfaces
from granian.server.embed import Server
from yarl import URL


@pytest.fixture
async def echo_server() -> AsyncGenerator[URL]:
    async def app(scope, receive, send):
        assert scope['type'] == 'http'
        payload = {
            "method": scope['method'],
            "path": scope['path'],
            "query_string": scope['query_string'].decode('utf-8'),
        }

        await send({
            'type': 'http.response.start',
            'status': 200,
            'headers': [
                [b'content-type', b'application/json'],
            ],
        })
        await send({
            'type': 'http.response.body',
            'body': json.dumps(payload).encode('utf-8'),
        })

    port = find_free_port()
    server = Server(app, port=port, interface=Interfaces.ASGINL)
    server_task = asyncio.create_task(server.serve())
    try:
        yield URL(f"http://localhost:{port}")
    finally:
        server.stop()
        await server_task


def find_free_port() -> int:
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(('', 0))
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        return s.getsockname()[1]
