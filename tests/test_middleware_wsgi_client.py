import json
from collections.abc import Callable, Generator
from typing import Any

import pytest
from pyreqwest.client import SyncClient, SyncClientBuilder
from pyreqwest.middleware.wsgi import WSGITestMiddleware
from pyreqwest.request import Request


def simple_wsgi_app(
    environ: dict[str, Any], start_response: Callable[..., Callable[[bytes], None]]
) -> Generator[bytes, None, None]:
    path = environ.get("PATH_INFO", "/")

    if path == "/":
        start_response("200 OK", [("Content-Type", "application/json")])
        yield b'{"message": "Hello World"}'
    elif path.startswith("/param/"):
        param = path.split("/")[-1]
        start_response("200 OK", [("Content-Type", "application/json")])
        yield f'{{"param": "{param}"}}'.encode()
    elif path == "/echo":
        method = environ["REQUEST_METHOD"]
        headers = []
        for k, v in environ.items():
            if k.startswith("HTTP_"):
                headers.append((k[5:].lower().replace("_", "-"), v))
            elif k in ("CONTENT_TYPE", "CONTENT_LENGTH"):
                headers.append((k.lower().replace("_", "-"), v))

        query_string = environ.get("QUERY_STRING", "")
        body = environ["wsgi.input"].read().decode()

        resp = {"method": method}
        if headers:
            # Sort headers for stable tests
            resp["headers"] = sorted(headers)
        if query_string:
            resp["query_string"] = query_string
        if body:
            resp["body"] = body

        start_response("200 OK", [("Content-Type", "application/json")])
        yield json.dumps(resp).encode()
    elif path == "/error":
        start_response("404 Not Found", [("Content-Type", "application/json")])
        yield b'{"detail": "Not found"}'
    elif path == "/stream":
        start_response("200 OK", [("Content-Type", "text/plain")])
        yield b"chunk_0_"
        yield b"chunk_1_"
        yield b"chunk_2_"
    else:
        start_response("404 Not Found", [("Content-Type", "text/plain")])
        yield b"Not found"


@pytest.fixture
def wsgi_client() -> Generator[SyncClient, None, None]:
    middleware = WSGITestMiddleware(simple_wsgi_app)
    with SyncClientBuilder().base_url("http://localhost").with_middleware(middleware).build() as client:
        yield client


def test_get_root(wsgi_client: SyncClient):
    response = wsgi_client.get("/").build().send()
    assert response.status == 200
    data = response.json()
    assert data == {"message": "Hello World"}


def test_get_with_path_params(wsgi_client: SyncClient):
    response = wsgi_client.get("/param/42").build().send()
    assert response.status == 200
    data = response.json()
    assert data == {"param": "42"}


def test_post_json(wsgi_client: SyncClient):
    request_data = {"name": "John Doe", "email": "john@example.com"}
    response = wsgi_client.post("/echo").body_json(request_data).build().send()
    assert response.status == 200
    data = response.json()
    assert data["method"] == "POST"
    assert "headers" in data

    assert json.loads(data["body"]) == request_data


def test_put_json(wsgi_client: SyncClient):
    response = wsgi_client.put("/echo").body_json({"name": "Jane Doe"}).build().send()
    assert response.status == 200
    data = response.json()
    assert data["method"] == "PUT"
    assert "headers" in data

    assert json.loads(data["body"]) == {"name": "Jane Doe"}


def test_headers(wsgi_client: SyncClient):
    response = (
        wsgi_client.get("/echo")
        .header("X-Header-1", "value1")
        .header("X-Header-2", "value2")
        .header("X-Header-2", "value3")
        .build()
        .send()
    )
    assert response.status == 200
    headers = response.json()["headers"]
    assert ["x-header-1", "value1"] in headers
    # WSGI concatenates multiple headers of the same name with commas
    assert ["x-header-2", "value2,value3"] in headers


def test_query_parameters(wsgi_client: SyncClient):
    response = wsgi_client.get("/echo").query([("k1", "v1"), ("k2", "v2"), ("k1", "v3")]).build().send()
    assert response.status == 200
    assert response.json()["query_string"] == "k1=v1&k2=v2&k1=v3"


def test_error_response(wsgi_client: SyncClient):
    response = wsgi_client.get("/error").build().send()
    assert response.status == 404
    data = response.json()
    assert data["detail"] == "Not found"


def test_streaming(wsgi_client: SyncClient):
    with wsgi_client.post("/stream").build_streamed() as response:
        assert response.status == 200

        assert response.body_reader.read_chunk() == b"chunk_0_"
        assert response.body_reader.read_chunk() == b"chunk_1_"
        assert response.body_reader.read_chunk() == b"chunk_2_"
        assert response.body_reader.read_chunk() is None


def test_scope_override():
    def scope_update(environ: dict[str, Any], request: Request) -> None:
        assert request.extensions["test"] == "something"
        assert environ.get("HTTP_X_TEST_HEADER") == "test-value"
        environ["HTTP_X_ADDED_HEADER"] = "added-value"

    middleware = WSGITestMiddleware(simple_wsgi_app, scope_update=scope_update)
    with SyncClientBuilder().base_url("http://localhost").with_middleware(middleware).build() as client:
        req = client.get("/echo").header("X-Test-Header", "test-value").build()
        req.extensions["test"] = "something"
        resp = req.send()
        assert resp.status == 200

        headers = resp.json()["headers"]
        assert ["x-test-header", "test-value"] in headers
        assert ["x-added-header", "added-value"] in headers
