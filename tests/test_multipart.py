import re
from collections.abc import AsyncGenerator, Iterator
from typing import Any

import pytest
from pyreqwest.client import Client, ClientBuilder, SyncClientBuilder
from pyreqwest.exceptions import BuilderError
from pyreqwest.http import Mime
from pyreqwest.multipart import FormBuilder, PartBuilder
from requests_toolbelt import MultipartDecoder  # type: ignore[import-untyped]

from tests.servers.server_subprocess import SubprocessServer
from tests.utils import temp_file


@pytest.fixture
async def client() -> AsyncGenerator[Client, None]:
    async with ClientBuilder().error_for_status(True).build() as client:
        yield client


def decode_multipart(echo_response: dict[str, Any]) -> MultipartDecoder:
    content_type = dict(echo_response["headers"])["content-type"]
    body_parts = "".join(echo_response["body_parts"])
    return MultipartDecoder(body_parts.encode("utf8"), content_type)


async def test_multipart_text_fields(client: Client, echo_server: SubprocessServer):
    form = FormBuilder().text("name", "John").text("email", "john@example.com")
    boundary = form.boundary
    resp = await client.post(echo_server.url).multipart(form).build().send()

    response_data = await resp.json()
    decoder = decode_multipart(response_data)
    assert len(decoder.parts) == 2

    name_part = next(p for p in decoder.parts if b'name="name"' in p.headers[b"content-disposition"])
    email_part = next(p for p in decoder.parts if b'name="email"' in p.headers[b"content-disposition"])

    assert name_part.content == b"John"
    assert email_part.content == b"john@example.com"
    assert ["content-type", f"multipart/form-data; boundary={boundary}"] in response_data["headers"]


async def test_multipart_with_custom_part(client: Client, echo_server: SubprocessServer):
    custom_part = PartBuilder.from_text("Custom content").mime("text/plain").file_name("custom.txt")

    form = FormBuilder().text("description", "File upload test").part("file", custom_part)

    resp = await client.post(echo_server.url).multipart(form).build().send()
    response_data = await resp.json()
    decoder = decode_multipart(response_data)

    assert len(decoder.parts) == 2

    desc_part = next(p for p in decoder.parts if b'name="description"' in p.headers[b"content-disposition"])
    file_part = next(p for p in decoder.parts if b'name="file"' in p.headers[b"content-disposition"])

    assert desc_part.content == b"File upload test"
    assert file_part.content == b"Custom content"
    assert b'filename="custom.txt"' in file_part.headers[b"content-disposition"]
    assert file_part.headers[b"content-type"] == b"text/plain"


@pytest.mark.parametrize("file", ["async", "sync"])
@pytest.mark.parametrize("req", ["async", "sync"])
async def test_multipart_with_file_upload(echo_server: SubprocessServer, file: str, req: str):
    test_content = "This is test file content"
    form = FormBuilder().text("title", "File Upload Test")

    with temp_file(test_content.encode(), suffix=".txt") as tmp_path:
        if file == "async":
            form = await form.file("document", tmp_path)
        else:
            assert file == "sync"
            form = form.sync_file("document", tmp_path)

    if req == "async":
        async with ClientBuilder().error_for_status(True).build() as client:
            resp = await client.post(echo_server.url).multipart(form).build().send()
            response_data = await resp.json()
    else:
        assert req == "sync"
        with SyncClientBuilder().error_for_status(True).build() as client:
            response_data = client.post(echo_server.url).multipart(form).build().send().json()

    decoder = decode_multipart(response_data)

    assert len(decoder.parts) == 2

    title_part = next(p for p in decoder.parts if b'name="title"' in p.headers[b"content-disposition"])
    doc_part = next(p for p in decoder.parts if b'name="document"' in p.headers[b"content-disposition"])

    assert title_part.content == b"File Upload Test"
    assert doc_part.content.decode() == test_content
    assert tmp_path.name.encode() in doc_part.headers[b"content-disposition"]


@pytest.mark.parametrize("file", ["async", "sync"])
@pytest.mark.parametrize("req", ["async", "sync"])
async def test_multipart_with_part_file(echo_server: SubprocessServer, file: str, req: str):
    test_content = "Part file content with special chars: àáâãäåæç"

    with temp_file(test_content.encode(), suffix=".txt") as tmp:
        if file == "async":
            file_part = (await PartBuilder.from_file(tmp)).mime("text/plain; charset=utf-8")
        else:
            assert file == "sync"
            file_part = PartBuilder.from_sync_file(tmp).mime("text/plain; charset=utf-8")

    form = FormBuilder().text("description", "Using Part.file").part("attachment", file_part)

    if req == "async":
        async with ClientBuilder().error_for_status(True).build() as client:
            resp = await client.post(echo_server.url).multipart(form).build().send()
            response_data = await resp.json()
    else:
        assert req == "sync"
        with SyncClientBuilder().error_for_status(True).build() as client:
            response_data = client.post(echo_server.url).multipart(form).build().send().json()

    decoder = decode_multipart(response_data)

    assert len(decoder.parts) == 2

    desc_part = next(p for p in decoder.parts if b'name="description"' in p.headers[b"content-disposition"])
    file_part = next(p for p in decoder.parts if b'name="attachment"' in p.headers[b"content-disposition"])

    assert desc_part.content == b"Using Part.file"
    assert file_part.content.decode("utf-8") == test_content
    assert tmp.name.encode() in file_part.headers[b"content-disposition"]
    assert file_part.headers[b"content-type"] == b"text/plain; charset=utf-8"


@pytest.mark.parametrize("sync", [False, True])
@pytest.mark.parametrize("with_length", [False, True])
async def test_multipart_with_stream_part(client: Client, echo_server: SubprocessServer, sync: bool, with_length: bool):
    if sync:

        def data_stream_sync() -> Iterator[bytes]:
            yield b"First chunk"
            yield b" Second chunk"

        if with_length:
            stream_part = PartBuilder.from_stream_with_length(data_stream_sync(), length=24)
        else:
            stream_part = PartBuilder.from_stream(data_stream_sync())
    else:

        async def data_stream() -> AsyncGenerator[bytes, None]:
            yield b"First chunk"
            yield b" Second chunk"

        if with_length:
            stream_part = PartBuilder.from_stream_with_length(data_stream(), length=24)
        else:
            stream_part = PartBuilder.from_stream(data_stream())

    stream_part = stream_part.mime("application/octet-stream").file_name("data.bin")
    form = FormBuilder().text("type", "streaming").part("data", stream_part)

    resp = await client.post(echo_server.url).multipart(form).build().send()
    response_data = await resp.json()
    decoder = decode_multipart(response_data)

    assert len(decoder.parts) == 2

    type_part = next(p for p in decoder.parts if b'name="type"' in p.headers[b"content-disposition"])
    data_part = next(p for p in decoder.parts if b'name="data"' in p.headers[b"content-disposition"])

    assert type_part.content == b"streaming"
    assert data_part.content == b"First chunk Second chunk"
    assert b'filename="data.bin"' in data_part.headers[b"content-disposition"]
    assert data_part.headers[b"content-type"] == b"application/octet-stream"


async def test_multipart_with_async_stream_async_part_in_sync_request(echo_server: SubprocessServer):
    async def data_stream() -> AsyncGenerator[bytes, None]:
        yield b""

    stream_part = PartBuilder.from_stream(data_stream()).mime("application/octet-stream").file_name("streamed_data.bin")
    form = FormBuilder().text("type", "streaming").part("data", stream_part)
    req_builder = SyncClientBuilder().build().post(echo_server.url)
    with pytest.raises(BuilderError, match=re.escape("Can not use async multipart (stream) in a blocking request")):
        req_builder.multipart(form)


async def test_multipart_with_bytes_part(client: Client, echo_server: SubprocessServer):
    binary_data = b"Binary content \x00\x01\x02"
    part = PartBuilder.from_bytes(binary_data).mime("application/octet-stream")
    form = FormBuilder().text("type", "binary").part("data", part)

    resp = await client.post(echo_server.url).multipart(form).build().send()
    response_data = await resp.json()
    decoder = decode_multipart(response_data)

    assert len(decoder.parts) == 2

    type_part = next(p for p in decoder.parts if b'name="type"' in p.headers[b"content-disposition"])
    data_part = next(p for p in decoder.parts if b'name="data"' in p.headers[b"content-disposition"])

    assert type_part.content == b"binary"
    assert data_part.content == binary_data
    assert data_part.headers[b"content-type"] == b"application/octet-stream"


async def test_multipart_with_headers(client: Client, echo_server: SubprocessServer):
    headers = {"X-Custom-Header": "custom-value", "X-Test": "test"}
    part = PartBuilder.from_text("content with headers").headers(headers)
    form = FormBuilder().part("custom", part)

    resp = await client.post(echo_server.url).multipart(form).build().send()
    response_data = await resp.json()
    decoder = decode_multipart(response_data)

    assert len(decoder.parts) == 1

    custom_part = decoder.parts[0]
    assert custom_part.content == b"content with headers"
    assert b'name="custom"' in custom_part.headers[b"content-disposition"]
    assert custom_part.headers[b"x-custom-header"] == b"custom-value"
    assert custom_part.headers[b"x-test"] == b"test"


async def test_multipart_encoding_options(client: Client, echo_server: SubprocessServer):
    special_value = "test/path?query=value&other=data"

    form1 = FormBuilder().text("data", special_value).percent_encode_path_segment()
    form2 = FormBuilder().text("data", special_value).percent_encode_attr_chars()
    form3 = FormBuilder().text("data", special_value).percent_encode_noop()

    resp1 = await client.post(echo_server.url).multipart(form1).build().send()
    resp2 = await client.post(echo_server.url).multipart(form2).build().send()
    resp3 = await client.post(echo_server.url).multipart(form3).build().send()

    decoder1 = decode_multipart(await resp1.json())
    decoder2 = decode_multipart(await resp2.json())
    decoder3 = decode_multipart(await resp3.json())

    assert len(decoder1.parts) == 1
    assert len(decoder2.parts) == 1
    assert len(decoder3.parts) == 1

    assert b'name="data"' in decoder1.parts[0].headers[b"content-disposition"]
    assert b'name="data"' in decoder2.parts[0].headers[b"content-disposition"]
    assert b'name="data"' in decoder3.parts[0].headers[b"content-disposition"]

    assert decoder1.parts[0].content == special_value.encode("utf-8")
    assert decoder2.parts[0].content == special_value.encode("utf-8")
    assert decoder3.parts[0].content == special_value.encode("utf-8")


async def test_multipart_empty_form(client: Client, echo_server: SubprocessServer):
    form = FormBuilder()
    boundary = form.boundary

    resp = await client.post(echo_server.url).multipart(form).build().send()
    response_data = await resp.json()

    assert ["content-type", f"multipart/form-data; boundary={boundary}"] in response_data["headers"]
    assert response_data["body_parts"] == []


async def test_multipart_multiple_values_same_name(client: Client, echo_server: SubprocessServer):
    form = FormBuilder().text("tags", "python").text("tags", "async").text("tags", "http")

    resp = await client.post(echo_server.url).multipart(form).build().send()
    response_data = await resp.json()
    decoder = decode_multipart(response_data)

    assert len(decoder.parts) == 3

    tag_parts = [p for p in decoder.parts if b'name="tags"' in p.headers[b"content-disposition"]]
    assert len(tag_parts) == 3

    assert [part.content.decode("utf-8") for part in tag_parts] == ["python", "async", "http"]


async def test_multipart_with_body_conflict(client: Client, echo_server: SubprocessServer):
    form = FormBuilder().text("test", "value")

    with pytest.raises(BuilderError, match="Can not set body when multipart or form is used"):
        client.post(echo_server.url).multipart(form).body_text("conflict").build()

    form2 = FormBuilder().text("test", "value")
    with pytest.raises(BuilderError, match="Can not set body when multipart or form is used"):
        client.post(echo_server.url).body_text("conflict").multipart(form2).build()


async def test_multipart_boundary_uniqueness():
    form = FormBuilder()
    assert form.boundary != FormBuilder().boundary
    assert form.boundary == form.boundary


async def test_multipart_form_chaining():
    form = FormBuilder()
    result = form.text("test", "value")
    assert result is form


async def test_part_chaining():
    part = PartBuilder.from_text("content")
    result = part.mime(Mime.parse("text/plain")).file_name("test.txt")
    assert result is part
