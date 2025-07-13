import asyncio
import json
import socket
from contextlib import closing
from typing import AsyncGenerator

import orjson
import pytest
from granian.constants import Interfaces
from granian.server.embed import Server

from pyreqwest.http import Url


@pytest.fixture
async def echo_server() -> AsyncGenerator[Url]:
    async def app(scope, receive, send):
        assert scope['type'] == 'http'

        body_parts = []
        more_body = True
        while more_body:
            message = await receive()
            if message.get('body', None):
                body_parts.append(message['body'])
            more_body = message.get('more_body', False)

        resp = {
            "headers": scope['headers'],
            "http_version": scope['http_version'],
            "method": scope['method'],
            "path": scope['path'],
            "query_string": scope['query_string'],
            "raw_path": scope['raw_path'],
            "scheme": scope['scheme'],
            "body_parts": body_parts,
        }

        def default(obj):
            if isinstance(obj, bytes):
                return obj.decode('utf-8')
            raise TypeError

        await send({
            'type': 'http.response.start',
            'status': 200,
            'headers': [
                [b'content-type', b'application/json'],
            ],
        })
        await send({
            'type': 'http.response.body',
            'body': orjson.dumps(resp, default=default),
        })

    port = find_free_port()
    server = Server(app, port=port, interface=Interfaces.ASGINL)
    server_task = asyncio.create_task(server.serve())
    try:
        yield Url(f"http://localhost:{port}")
    finally:
        server.stop()
        await server_task


def find_free_port() -> int:
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(('', 0))
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        return s.getsockname()[1]
