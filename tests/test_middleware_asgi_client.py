from typing import AsyncGenerator

import pytest
from starlette.applications import Starlette
from starlette.routing import Route
from starlette.requests import Request
from starlette.responses import JSONResponse

from pyreqwest.client import ClientBuilder, Client
from pyreqwest.middleware.asgi import ASGITestMiddleware


@pytest.fixture
def starlette_app():
    async def root(request: Request):
        return JSONResponse({"message": "Hello World"})

    async def get_user(request: Request):
        user_id = int(request.path_params["user_id"])
        return JSONResponse({"user_id": user_id, "name": f"User {user_id}"})

    async def create_user(request: Request):
        body = await request.json()
        return JSONResponse({"id": 123, "name": body["name"], "email": body["email"]})

    async def update_user(request: Request):
        user_id = int(request.path_params["user_id"])
        body = await request.json()
        return JSONResponse({"id": user_id, "name": body["name"], "updated": True})

    async def delete_user(request: Request):
        user_id = int(request.path_params["user_id"])
        return JSONResponse({"deleted": True, "user_id": user_id})

    async def get_headers(request: Request):
        return JSONResponse({"headers": dict(request.headers)})

    async def get_query(request: Request):
        return JSONResponse({"query": dict(request.query_params)})

    async def error_endpoint(request: Request):
        return JSONResponse({"detail": "Not found"}, status_code=404)

    routes = [
        Route("/", root, methods=["GET"]),
        Route("/users/{user_id:int}", get_user, methods=["GET"]),
        Route("/users", create_user, methods=["POST"]),
        Route("/users/{user_id:int}", update_user, methods=["PUT"]),
        Route("/users/{user_id:int}", delete_user, methods=["DELETE"]),
        Route("/headers", get_headers, methods=["GET"]),
        Route("/query", get_query, methods=["GET"]),
        Route("/error", error_endpoint, methods=["GET"]),
    ]
    return Starlette(routes=routes)


@pytest.fixture
async def asgi_client(starlette_app: Starlette) -> AsyncGenerator[Client]:
    middleware = ASGITestMiddleware(starlette_app)
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
