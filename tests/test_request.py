from typing import AsyncGenerator

import pytest

from pyreqwest.client import ClientBuilder, Client
from pyreqwest.exceptions import StatusError
from pyreqwest.http import Url
from .servers.echo_body_parts_server import EchoBodyPartsServer
from .servers.echo_server import EchoServer


@pytest.mark.parametrize("value", [True, False])
async def test_error_for_status(echo_server: EchoServer, value: bool):
    url = Url(str(echo_server.address))
    url.set_query_dict({"status": str(400)})

    async with ClientBuilder().error_for_status(False).build() as client:
        req = client.get(url).error_for_status(value).build_consumed()
        if value:
            with pytest.raises(StatusError) as e:
                await req.send()
            assert e.value.details["status"] == 400
        else:
            assert (await req.send()).status == 400


@pytest.fixture
async def client() -> AsyncGenerator[Client, None]:
    async with ClientBuilder().error_for_status(True).build() as client:
        yield client


async def test_stream(client: Client, echo_body_parts_server: EchoBodyPartsServer):
    async def stream_gen():
        for i in range(5):
            yield f"part {i}".encode("utf-8")

    async with client.post(echo_body_parts_server.address).body_stream(stream_gen()).build_streamed() as req:
        chunks = []
        while chunk := await req.next_chunk():
            chunks.append(chunk)
        assert chunks == [b"part 0", b"part 1", b"part 2", b"part 3", b"part 4"]
