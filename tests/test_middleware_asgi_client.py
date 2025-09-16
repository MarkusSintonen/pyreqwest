from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager
from typing import Any

import pytest
from pyreqwest.client import Client, ClientBuilder
from pyreqwest.middleware.asgi import ASGITestMiddleware
from pyreqwest.request import Request
from starlette.applications import Starlette
from starlette.requests import Request as StarletteRequest
from starlette.responses import JSONResponse, StreamingResponse
from starlette.routing import Route


@pytest.fixture
def starlette_app():
    async def root_endpoint(_request: StarletteRequest) -> JSONResponse:
        return JSONResponse({"message": "Hello World"})

    async def get_param(request: StarletteRequest) -> JSONResponse:
        return JSONResponse({"param": request.path_params["param"]})

    async def echo_endpoint(request: StarletteRequest) -> JSONResponse:
        resp: dict[str, Any] = {"method": request.method}
        if headers := [*request.headers.items()]:
            resp["headers"] = headers
        if query_string := request.scope["query_string"].decode():
            resp["query_string"] = query_string
        if body := (await request.body()).decode():
            resp["body"] = body
        return JSONResponse(resp)

    async def error_endpoint(_request: StarletteRequest) -> JSONResponse:
        return JSONResponse({"detail": "Not found"}, status_code=404)

    async def streaming_endpoint(request: StarletteRequest) -> StreamingResponse:
        received_chunks = [chunk.decode() async for chunk in request.stream() if chunk]

        async def generate_response() -> AsyncGenerator[str]:
            for c in received_chunks:
                yield f"echo_{c}"

        return StreamingResponse(generate_response(), media_type="text/plain")

    routes = [
        Route("/", root_endpoint, methods=["GET"]),
        Route("/param/{param:int}", get_param, methods=["GET"]),
        Route("/echo", echo_endpoint, methods=["GET", "POST", "PUT", "DELETE"]),
        Route("/error", error_endpoint, methods=["GET"]),
        Route("/stream", streaming_endpoint, methods=["POST"]),
    ]
    return Starlette(routes=routes)


@pytest.fixture
async def asgi_client(starlette_app: Starlette) -> AsyncGenerator[Client]:
    middleware = ASGITestMiddleware(starlette_app)
    async with middleware, ClientBuilder().base_url("http://localhost").with_middleware(middleware).build() as client:
        yield client


async def test_get_root(asgi_client: Client):
    response = await asgi_client.get("/").build().send()
    assert response.status == 200
    data = await response.json()
    assert data == {"message": "Hello World"}


async def test_get_with_path_params(asgi_client: Client):
    response = await asgi_client.get("/param/42").build().send()
    assert response.status == 200
    data = await response.json()
    assert data == {"param": 42}


async def test_post_json(asgi_client: Client):
    request_data = {"name": "John Doe", "email": "john@example.com"}
    response = await asgi_client.post("/echo").body_json(request_data).build().send()
    assert response.status == 200
    assert await response.json() == {
        "body": '{"email":"john@example.com","name":"John Doe"}',
        "headers": [["content-type", "application/json"]],
        "method": "POST",
    }


async def test_put_json(asgi_client: Client):
    response = await asgi_client.put("/echo").body_json({"name": "Jane Doe"}).build().send()
    assert response.status == 200
    assert await response.json() == {
        "body": '{"name":"Jane Doe"}',
        "headers": [["content-type", "application/json"]],
        "method": "PUT",
    }


async def test_headers(asgi_client: Client):
    response = await (
        asgi_client.get("/echo")
        .header("X-Header-1", "value1")
        .header("X-Header-2", "value2")
        .header("X-Header-2", "value3")
        .build()
        .send()
    )
    assert response.status == 200
    assert (await response.json())["headers"] == [
        ["x-header-1", "value1"],
        ["x-header-2", "value2"],
        ["x-header-2", "value3"],
    ]


async def test_query_parameters(asgi_client: Client):
    response = await asgi_client.get("/echo").query([("k1", "v1"), ("k2", "v2"), ("k1", "v3")]).build().send()
    assert response.status == 200
    assert (await response.json())["query_string"] == "k1=v1&k2=v2&k1=v3"


async def test_error_response(asgi_client: Client):
    response = await asgi_client.get("/error").build().send()
    assert response.status == 404
    data = await response.json()
    assert data["detail"] == "Not found"


async def test_streaming(asgi_client: Client):
    async def generate_stream() -> AsyncGenerator[bytes]:
        for i in range(3):
            yield f"data_chunk_{i}_".encode()

    async with asgi_client.post("/stream").body_stream(generate_stream()).build_streamed() as response:
        assert response.status == 200

        assert await response.body_reader.read_chunk() == b"echo_data_chunk_0_"
        assert await response.body_reader.read_chunk() == b"echo_data_chunk_1_"
        assert await response.body_reader.read_chunk() == b"echo_data_chunk_2_"
        assert await response.body_reader.read_chunk() is None


async def test_scope_override(starlette_app: Starlette):
    async def scope_update(scope: dict[str, Any], request: Request) -> None:
        assert request.extensions["test"] == "something"
        assert [b"x-test-header", b"test-value"] in scope["headers"]
        scope["headers"].append([b"x-added-header", b"added-value"])

    middleware = ASGITestMiddleware(starlette_app, scope_update=scope_update)
    async with ClientBuilder().base_url("http://localhost").with_middleware(middleware).build() as client:
        req = client.get("/echo").header("X-Test-Header", "test-value").build()
        req.extensions["test"] = "something"
        resp = await req.send()
        assert resp.status == 200
        assert (await resp.json())["headers"] == [["x-test-header", "test-value"], ["x-added-header", "added-value"]]


async def test_lifespan_events():
    startup_called = False
    shutdown_called = False

    @asynccontextmanager
    async def lifespan(_app: Starlette) -> AsyncGenerator[dict[str, Any]]:
        nonlocal startup_called, shutdown_called
        startup_called = True
        yield {"my_state": "some state"}
        shutdown_called = True

    async def root(request: StarletteRequest) -> JSONResponse:
        return JSONResponse({"server_state": request.state.my_state})

    middleware = ASGITestMiddleware(Starlette(lifespan=lifespan, routes=[Route("/", root, methods=["GET"])]))

    assert not startup_called
    assert not shutdown_called

    async with middleware:
        assert startup_called
        assert not shutdown_called

        async with ClientBuilder().with_middleware(middleware).build() as client:
            response = await client.get("http://localhost/").build().send()
            assert response.status == 200
            assert await response.json() == {"server_state": "some state"}

    assert startup_called
    assert shutdown_called


async def test_lifespan_failure__startup():
    @asynccontextmanager
    async def failing_lifespan(_app: Starlette) -> AsyncGenerator[dict[str, Any]]:
        raise RuntimeError("Lifespan failure")
        yield

    middleware = ASGITestMiddleware(Starlette(lifespan=failing_lifespan))

    with pytest.raises(RuntimeError, match="Lifespan failure"):
        await middleware.__aenter__()

    with pytest.raises(RuntimeError, match="Lifespan failure"):
        await middleware.__aenter__()


async def test_lifespan_failure__shutdown():
    @asynccontextmanager
    async def failing_lifespan(_app: Starlette) -> AsyncGenerator[dict[str, Any]]:
        yield {}
        raise RuntimeError("Lifespan failure")

    middleware = ASGITestMiddleware(Starlette(lifespan=failing_lifespan))

    await middleware.__aenter__()
    with pytest.raises(RuntimeError, match="Lifespan failure"):
        await middleware.__aexit__(None, None, None)

    await middleware.__aenter__()
    with pytest.raises(RuntimeError, match="Lifespan failure"):
        await middleware.__aexit__(None, None, None)
