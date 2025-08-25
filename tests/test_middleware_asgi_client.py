from contextlib import asynccontextmanager
from typing import AsyncGenerator, Any

import pytest
from starlette.applications import Starlette
from starlette.routing import Route
from starlette.requests import Request as StarletteRequest
from starlette.responses import JSONResponse, StreamingResponse

from pyreqwest.client import ClientBuilder, Client
from pyreqwest.middleware.asgi import ASGITestMiddleware
from pyreqwest.request import Request


@pytest.fixture
def starlette_app():
    async def root(request: StarletteRequest):
        return JSONResponse({"message": "Hello World"})

    async def get_user(request: StarletteRequest):
        user_id = int(request.path_params["user_id"])
        return JSONResponse({"user_id": user_id, "name": f"User {user_id}"})

    async def create_user(request: StarletteRequest):
        body = await request.json()
        return JSONResponse({"id": 123, "name": body["name"], "email": body["email"]})

    async def update_user(request: StarletteRequest):
        user_id = int(request.path_params["user_id"])
        body = await request.json()
        return JSONResponse({"id": user_id, "name": body["name"], "updated": True})

    async def delete_user(request: StarletteRequest):
        user_id = int(request.path_params["user_id"])
        return JSONResponse({"deleted": True, "user_id": user_id})

    async def get_headers(request: StarletteRequest):
        return JSONResponse({"headers": dict(request.headers)})

    async def get_query(request: StarletteRequest):
        return JSONResponse({"query": dict(request.query_params)})

    async def error_endpoint(request: StarletteRequest):
        return JSONResponse({"detail": "Not found"}, status_code=404)

    async def streaming_endpoint(request: StarletteRequest):
        received_chunks = []
        async for chunk in request.stream():
            if chunk:
                received_chunks.append(chunk.decode())

        async def generate_response():
            for c in received_chunks:
                yield f"echo_{c}"

        return StreamingResponse(generate_response(), media_type="text/plain")

    routes = [
        Route("/", root, methods=["GET"]),
        Route("/users/{user_id:int}", get_user, methods=["GET"]),
        Route("/users", create_user, methods=["POST"]),
        Route("/users/{user_id:int}", update_user, methods=["PUT"]),
        Route("/users/{user_id:int}", delete_user, methods=["DELETE"]),
        Route("/headers", get_headers, methods=["GET"]),
        Route("/query", get_query, methods=["GET"]),
        Route("/error", error_endpoint, methods=["GET"]),
        Route("/stream", streaming_endpoint, methods=["POST"]),
    ]
    return Starlette(routes=routes)


@pytest.fixture
async def asgi_client(starlette_app: Starlette) -> AsyncGenerator[Client]:
    middleware = ASGITestMiddleware(starlette_app)
    async with middleware:
        async with ClientBuilder().base_url("http://localhost").with_middleware(middleware).build() as client:
            yield client


async def test_get_root(asgi_client: Client):
    response = await asgi_client.get("/").build_consumed().send()
    assert response.status == 200
    data = await response.json()
    assert data == {"message": "Hello World"}


async def test_get_with_path_params(asgi_client: Client):
    response = await asgi_client.get("/users/42").build_consumed().send()
    assert response.status == 200
    data = await response.json()
    assert data == {"user_id": 42, "name": "User 42"}


async def test_post_json(asgi_client: Client):
    request_data = {"name": "John Doe", "email": "john@example.com"}
    response = await (asgi_client.post("/users")
                     .body_json(request_data)
                     .build_consumed()
                     .send())
    assert response.status == 200
    data = await response.json()
    assert data == {"id": 123, "name": "John Doe", "email": "john@example.com"}


async def test_put_json(asgi_client: Client):
    request_data = {"name": "Jane Doe"}
    response = await (asgi_client.put("/users/456")
                     .body_json(request_data)
                     .build_consumed()
                     .send())
    assert response.status == 200
    data = await response.json()
    assert data == {"id": 456, "name": "Jane Doe", "updated": True}
    await asgi_client.close()


async def test_delete(asgi_client: Client):
    response = await asgi_client.delete("/users/789").build_consumed().send()
    assert response.status == 200
    data = await response.json()
    assert data == {"deleted": True, "user_id": 789}


async def test_headers(asgi_client: Client):
    response = await (asgi_client.get("/headers")
                     .header("X-Test-Header", "test-value")
                     .header("User-Agent", "pyreqwest-test")
                     .build_consumed()
                     .send())
    assert response.status == 200
    data = await response.json()
    headers = data["headers"]
    assert headers["x-test-header"] == "test-value"
    assert headers["user-agent"] == "pyreqwest-test"


async def test_query_parameters(asgi_client: Client):
    response = await (asgi_client.get("/query")
                     .query({"name": "test", "page": "2"})
                     .build_consumed()
                     .send())
    assert response.status == 200
    data = await response.json()
    query = data["query"]
    assert query["name"] == "test"
    assert query["page"] == "2"


async def test_error_response(asgi_client: Client):
    response = await asgi_client.get("/error").build_consumed().send()
    assert response.status == 404
    data = await response.json()
    assert data["detail"] == "Not found"


async def test_streaming(asgi_client: Client):
    async def generate_stream():
        for i in range(3):
            yield f"data_chunk_{i}_".encode()

    async with (asgi_client.post("/stream")
                .body_stream(generate_stream())
                .build_streamed()) as response:

        assert response.status == 200

        assert await response.next_chunk() == "echo_data_chunk_0_".encode()
        assert await response.next_chunk() == "echo_data_chunk_1_".encode()
        assert await response.next_chunk() == "echo_data_chunk_2_".encode()
        assert await response.next_chunk() is None


async def test_scope_override(starlette_app: Starlette):
    async def scope_update(scope: dict[str, Any], request: Request) -> None:
        assert request.extensions["test"] == "something"
        assert [b"x-test-header", b"test-value"] in scope["headers"]
        scope["headers"].append([b"x-added-header", b"added-value"])

    middleware = ASGITestMiddleware(starlette_app, scope_update=scope_update)
    async with ClientBuilder().base_url("http://localhost").with_middleware(middleware).build() as client:
        req = client.get("/headers").header("X-Test-Header", "test-value").build_consumed()
        req.extensions["test"] = "something"
        resp = await req.send()
        assert resp.status == 200
        assert await resp.json() ==  {'headers': {'x-added-header': 'added-value', 'x-test-header': 'test-value'}}


async def test_lifespan_events():
    startup_called = False
    shutdown_called = False

    @asynccontextmanager
    async def lifespan(app: Starlette) -> AsyncGenerator[dict[str, Any]]:
        nonlocal startup_called, shutdown_called
        startup_called = True
        yield {"my_state": "some state"}
        shutdown_called = True

    async def root(request: StarletteRequest):
        return JSONResponse({"server_state": request.state.my_state})

    middleware = ASGITestMiddleware(Starlette(lifespan=lifespan, routes=[Route("/", root, methods=["GET"])]))

    assert not startup_called
    assert not shutdown_called

    async with middleware:
        assert startup_called
        assert not shutdown_called

        async with ClientBuilder().with_middleware(middleware).build() as client:
            response = await client.get("http://localhost/").build_consumed().send()
            assert response.status == 200
            assert await response.json() == {"server_state": "some state"}

    assert startup_called
    assert shutdown_called


async def test_lifespan_failure__startup():
    @asynccontextmanager
    async def failing_lifespan(app: Starlette) -> AsyncGenerator[dict[str, Any]]:
        raise RuntimeError("Lifespan failure")
        yield

    middleware = ASGITestMiddleware(Starlette(lifespan=failing_lifespan))

    with pytest.raises(RuntimeError, match="Lifespan failure"):
        await middleware.__aenter__()

    with pytest.raises(RuntimeError, match="Lifespan failure"):
        await middleware.__aenter__()


async def test_lifespan_failure__shutdown():
    @asynccontextmanager
    async def failing_lifespan(app: Starlette) -> AsyncGenerator[dict[str, Any]]:
        yield
        raise RuntimeError("Lifespan failure")

    middleware = ASGITestMiddleware(Starlette(lifespan=failing_lifespan))

    await middleware.__aenter__()
    with pytest.raises(RuntimeError, match="Lifespan failure"):
        await middleware.__aexit__(None, None, None)

    await middleware.__aenter__()
    with pytest.raises(RuntimeError, match="Lifespan failure"):
        await middleware.__aexit__(None, None, None)
