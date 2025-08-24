from typing import AsyncGenerator

import pytest
from fastapi import FastAPI, Request, HTTPException

from pyreqwest.client import ClientBuilder, Client
from pyreqwest.middleware.asgi import ASGITestMiddleware


@pytest.fixture
def fastapi_app():
    app = FastAPI()

    @app.get("/")
    async def root():
        return {"message": "Hello World"}

    @app.get("/users/{user_id}")
    async def get_user(user_id: int):
        return {"user_id": user_id, "name": f"User {user_id}"}

    @app.post("/users")
    async def create_user(request: Request):
        body = await request.json()
        return {"id": 123, "name": body["name"], "email": body["email"]}

    @app.put("/users/{user_id}")
    async def update_user(user_id: int, request: Request):
        body = await request.json()
        return {"id": user_id, "name": body["name"], "updated": True}

    @app.delete("/users/{user_id}")
    async def delete_user(user_id: int):
        return {"deleted": True, "user_id": user_id}

    @app.get("/headers")
    async def get_headers(request: Request):
        return {"headers": dict(request.headers)}

    @app.get("/query")
    async def get_query(request: Request):
        return {"query": dict(request.query_params)}

    @app.get("/error")
    async def error_endpoint():
        raise HTTPException(status_code=404, detail="Not found")

    return app


@pytest.fixture
async def asgi_client(fastapi_app: FastAPI) -> AsyncGenerator[Client]:
    middleware = ASGITestMiddleware(fastapi_app)
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
