from typing import AsyncGenerator

import pytest

from pyreqwest.client import ClientBuilder, Client
from pyreqwest.exceptions import StatusError
from pyreqwest.http import Url
from pyreqwest.request import StreamRequest
from .servers.server import Server


@pytest.mark.parametrize("value", [True, False])
async def test_error_for_status(echo_server: Server, value: bool):
    url = Url(str(echo_server.url))
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


async def test_build_consumed(client: Client, echo_body_parts_server: Server):
    sent = "a" * (StreamRequest.default_initial_read_size() * 3)
    resp = await client.post(echo_body_parts_server.url).body_text(sent).build_consumed().send()
    assert (await resp.text()) == sent


async def test_build_streamed(client: Client, echo_body_parts_server: Server):
    sent = "a" * (StreamRequest.default_initial_read_size() * 3)
    async with client.post(echo_body_parts_server.url).body_text(sent).build_streamed() as resp:
        assert (await resp.text()) == sent
