import json
import re
from collections.abc import AsyncGenerator
from typing import Any

import pytest
from dirty_equals import Contains, IsPartialDict, IsStr
from pyreqwest.client import ClientBuilder
from pyreqwest.pytest_plugin import ClientMocker
from pyreqwest.request import Request
from pyreqwest.response import Response, ResponseBuilder

from tests.servers.server import Server

import_time_client = ClientBuilder().build()


async def test_simple_get_mock(client_mocker: ClientMocker) -> None:
    client_mocker.get("/api").with_body_text("Hello World")

    resp = await ClientBuilder().build().get("http://example.com/api").build_consumed().send()

    assert resp.status == 200
    assert await resp.text() == "Hello World"
    assert client_mocker.get_call_count() == 1


async def test_method_specific_mocks(client_mocker: ClientMocker) -> None:
    mock_get = client_mocker.get("/users").with_body_json({"users": []})
    mock_post = client_mocker.post("/users").with_status(201).with_body_json({"id": 123})
    mock_put = client_mocker.put("/users/123").with_status(202)
    mock_delete = client_mocker.delete("/users/123").with_status(204)

    client = ClientBuilder().build()

    get_resp = await client.get("http://api.example.com/users").build_consumed().send()
    assert get_resp.status == 200
    assert await get_resp.json() == {"users": []}

    post_resp = (
        await client.post("http://api.example.com/users")
        .body_text(json.dumps({"name": "John"}))
        .build_consumed()
        .send()
    )
    assert post_resp.status == 201
    assert await post_resp.json() == {"id": 123}

    put_resp = (
        await client.put("http://api.example.com/users/123")
        .body_text(json.dumps({"name": "Jane"}))
        .build_consumed()
        .send()
    )
    assert put_resp.status == 202

    for _ in range(2):
        delete_resp = await client.delete("http://api.example.com/users/123").build_consumed().send()
        assert delete_resp.status == 204

    assert client_mocker.get_call_count() == 5
    assert mock_get.get_call_count() == 1
    assert mock_post.get_call_count() == 1
    assert mock_put.get_call_count() == 1
    assert mock_delete.get_call_count() == 2


async def test_regex_path_matching(client_mocker: ClientMocker) -> None:
    pattern = re.compile(r"/users/\d+")
    client_mocker.strict(True).get(pattern).with_body_json({"id": 456, "name": "Test User"})

    client = ClientBuilder().build()

    resp1 = await client.get("http://api.example.com/users/123").build_consumed().send()
    resp2 = await client.get("http://api.example.com/users/456").build_consumed().send()
    with pytest.raises(AssertionError, match="No mock rule matched request"):
        await client.get("http://api.example.com/users/abc").build_consumed().send()

    assert await resp1.json() == {"id": 456, "name": "Test User"}
    assert await resp2.json() == {"id": 456, "name": "Test User"}
    assert client_mocker.get_call_count() == 2


async def test_header_matching(client_mocker: ClientMocker) -> None:
    client_mocker.post("/data").match_header("Authorization", "Bearer token123").with_status(200).with_body_text(
        "Authorized",
    )

    client_mocker.post("/data").with_status(401).with_body_text("Unauthorized")

    client = ClientBuilder().build()

    auth_resp = (
        await client.post("http://api.example.com/data")
        .header("Authorization", "Bearer token123")
        .build_consumed()
        .send()
    )
    assert auth_resp.status == 200
    assert await auth_resp.text() == "Authorized"

    unauth_resp = await client.post("http://api.example.com/data").build_consumed().send()
    assert unauth_resp.status == 401
    assert await unauth_resp.text() == "Unauthorized"


async def test_body_matching(client_mocker: ClientMocker) -> None:
    client_mocker.post("/echo").match_body('{"test": "data"}').with_body_text("JSON matched")

    client_mocker.post("/echo").match_body(b"binary data").with_body_text("Binary matched")

    client = ClientBuilder().build()

    json_resp = await client.post("http://api.example.com/echo").body_text('{"test": "data"}').build_consumed().send()
    assert await json_resp.text() == "JSON matched"

    binary_resp = await client.post("http://api.example.com/echo").body_bytes(b"binary data").build_consumed().send()
    assert await binary_resp.text() == "Binary matched"


async def test_regex_body_matching(client_mocker: ClientMocker) -> None:
    pattern = re.compile(r'.*"action":\s*"create".*')
    client_mocker.post("/actions").match_body(pattern).with_status(201).with_body_text("Create action processed")

    client = ClientBuilder().build()

    resp = (
        await client.post("http://api.example.com/actions")
        .body_text(json.dumps({"action": "create", "resource": "user"}))
        .build_consumed()
        .send()
    )

    assert resp.status == 201
    assert await resp.text() == "Create action processed"


async def test_request_capture(client_mocker: ClientMocker) -> None:
    get_mock = client_mocker.get("/test").with_body_text("response")
    post_mock = client_mocker.post("/test").with_body_text("posted")

    client = ClientBuilder().build()

    await client.get("http://api.example.com/test").header("User-Agent", "test-client").build_consumed().send()

    await client.post("http://api.example.com/test").body_text(json.dumps({"key": "value"})).build_consumed().send()

    all_requests = client_mocker.get_requests()
    assert len(all_requests) == 2

    get_requests = get_mock.get_requests()
    assert len(get_requests) == 1
    assert get_requests[0].method == "GET"
    assert get_requests[0].headers.get("User-Agent") == "test-client"

    post_requests = post_mock.get_requests()
    assert len(post_requests) == 1
    assert post_requests[0].method == "POST"


async def test_call_counting(client_mocker: ClientMocker) -> None:
    mock = client_mocker.get("/endpoint").with_body_text("response")

    client = ClientBuilder().build()

    for _ in range(3):
        await client.get("http://api.example.com/endpoint").build_consumed().send()

    assert client_mocker.get_call_count() == 3
    assert mock.get_call_count() == 3


async def test_response_headers(client_mocker: ClientMocker) -> None:
    client_mocker.get("/test").with_body_text("Hello").with_header("X-Custom-Header", "custom-value").with_header(
        "x-rate-limit",
        "100",
    )
    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/test").build_consumed().send()

    assert resp.headers["X-Custom-Header"] == "custom-value"
    assert resp.headers["X-Rate-Limit"] == "100"


async def test_json_response(client_mocker: ClientMocker) -> None:
    test_data = {"users": [{"id": 1, "name": "John"}, {"id": 2, "name": "Jane"}]}
    client_mocker.get("/users").with_body_json(test_data)

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/users").build_consumed().send()

    assert resp.headers["content-type"] == "application/json"
    assert await resp.json() == test_data


async def test_bytes_response(client_mocker: ClientMocker) -> None:
    test_data = b"binary data content"
    client_mocker.get("/binary").with_body_bytes(test_data)

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/binary").build_consumed().send()

    assert await resp.bytes() == test_data


async def test_strict_mode(client_mocker: ClientMocker) -> None:
    client_mocker.strict(True)
    client_mocker.get("/allowed").with_body_text("OK")

    client = ClientBuilder().build()

    resp = await client.get("http://api.example.com/allowed").build_consumed().send()
    assert await resp.text() == "OK"

    with pytest.raises(AssertionError, match="No mock rule matched request"):
        await client.get("http://api.example.com/forbidden").build_consumed().send()


async def test_reset_mocks(client_mocker: ClientMocker) -> None:
    client_mocker.get("/test").with_body_text("response")

    client = ClientBuilder().build()
    await client.get("http://api.example.com/test").build_consumed().send()

    assert client_mocker.get_call_count() == 1
    assert len(client_mocker.get_requests()) == 1

    client_mocker.reset_requests()

    assert client_mocker.get_call_count() == 0
    assert len(client_mocker.get_requests()) == 0


async def test_multiple_rules_first_match_wins(client_mocker: ClientMocker) -> None:
    client_mocker.get("/users/123").match_query({"param": "1"}).with_body_text("Specific user")
    client_mocker.get("/users/123").with_body_text("General user")

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/users/123?param=1").build_consumed().send()

    assert await resp.text() == "Specific user"


async def test_method_pattern_matching(client_mocker: ClientMocker) -> None:
    client_mocker.strict(True)
    client_mocker.mock(re.compile(r"GET|POST"), "/data").with_body_json({"message": "success"})

    client = ClientBuilder().build()

    get_resp = await client.get("http://api.example.com/data").build_consumed().send()
    assert get_resp.status == 200
    assert await get_resp.json() == {"message": "success"}

    post_resp = await client.post("http://api.example.com/data").build_consumed().send()
    assert post_resp.status == 200
    assert await post_resp.json() == {"message": "success"}

    req = client.put("http://api.example.com/data").build_consumed()
    with pytest.raises(AssertionError, match="No mock rule matched request"):
        await req.send()

    assert client_mocker.get_call_count() == 2


async def test_without_mocking_requests_pass_through(client_mocker: ClientMocker, echo_server: Server) -> None:
    client_mocker.get("/api").with_body_json({"mocked": True, "source": "mock"})

    client = ClientBuilder().build()

    mocked_resp = await client.get("http://mocked.example.com/api").build_consumed().send()
    assert mocked_resp.status == 200
    mocked_data = await mocked_resp.json()
    assert mocked_data["mocked"] is True
    assert mocked_data["source"] == "mock"

    real_resp = await client.get(echo_server.url).build_consumed().send()
    assert real_resp.status == 200
    real_data = await real_resp.json()

    assert "method" in real_data
    assert real_data["method"] == "GET"
    assert "headers" in real_data
    assert "body_parts" in real_data
    assert real_data.get("mocked") is None

    assert client_mocker.get_call_count() == 1


async def test_regex_header_matching(client_mocker: ClientMocker) -> None:
    client_mocker.post("/secure").match_header("Authorization", re.compile(r"Bearer \w+")).with_body_json(
        {"authenticated": True},
    )

    client = ClientBuilder().build()

    auth_resp = (
        await client.post("http://api.service.com/secure")
        .header("Authorization", "Bearer abc123xyz")
        .build_consumed()
        .send()
    )
    assert (await auth_resp.json())["authenticated"] is True


async def test_mock_chaining_and_reset(client_mocker: ClientMocker) -> None:
    client_mocker.get("/resource").with_status(200).with_body_json({"id": 1, "name": "Resource"}).with_header(
        "X-Rate-Limit",
        "100",
    ).with_header("X-Remaining", "99")

    client = ClientBuilder().build()

    resp = await client.get("http://api.service.com/resource").build_consumed().send()
    assert resp.status == 200
    assert resp.headers["X-Rate-Limit"] == "100"
    assert (await resp.json())["name"] == "Resource"

    assert client_mocker.get_call_count() == 1

    client_mocker.reset_requests()
    assert client_mocker.get_call_count() == 0
    assert len(client_mocker.get_requests()) == 0


@pytest.mark.parametrize(
    ("body_match", "matches"),
    [
        (b"part1part2", True),
        (b"part1", False),
        ("part1part2", True),
        ("part1", False),
        (re.compile(r"part1part2"), True),
        (re.compile(r"part1"), True),
        (re.compile(r"t1pa"), True),
        (re.compile(r"part3"), False),
    ],
)
async def test_stream_match(client_mocker: ClientMocker, body_match: Any, matches: bool) -> None:
    async def stream_generator() -> AsyncGenerator[bytes]:
        yield b"part1"
        yield b"part2"

    client_mocker.strict(True)
    mock = client_mocker.post("/stream").match_body(body_match).with_body_text("Stream received")

    client = ClientBuilder().error_for_status(True).build()
    req = client.post("http://api.example.com/stream").body_stream(stream_generator()).build_consumed()

    if matches:
        resp = await req.send()
        assert await resp.text() == "Stream received"
        assert len(client_mocker.get_requests()) == 1
        request = mock.get_requests()[0]
        assert request.method == "POST"
        assert request.url == "http://api.example.com/stream"
        assert request.body is not None and request.body.copy_bytes() == b"part1part2"
    else:
        with pytest.raises(AssertionError, match="No mock rule matched request"):
            await req.send()
        assert len(client_mocker.get_requests()) == 0


async def test_import_time_client_is_mocked(client_mocker: ClientMocker) -> None:
    client_mocker.get("/").with_body_text("Mocked response")

    resp = await import_time_client.get("http://foo.invalid").build_consumed().send()
    assert resp.status == 200
    assert (await resp.text()) == "Mocked response"
    assert client_mocker.get_call_count() == 1


async def test_custom_matcher_basic(client_mocker: ClientMocker) -> None:
    async def has_api_version(request: Request) -> bool:
        return request.headers.get("X-API-Version") == "v2"

    client_mocker.mock().match_request(has_api_version).with_body_text("API v2 response")
    client_mocker.get().with_body_text("Default response")

    client = ClientBuilder().build()

    v2_resp = await client.get("http://api.example.com/data").header("X-API-Version", "v2").build_consumed().send()
    assert await v2_resp.text() == "API v2 response"

    default_resp = await client.get("http://api.example.com/data").build_consumed().send()
    assert await default_resp.text() == "Default response"


async def test_custom_matcher_combined(client_mocker: ClientMocker) -> None:
    async def has_user_agent(request: Request) -> bool:
        return "TestClient" in request.headers.get("User-Agent", "")

    client_mocker.get("/protected").match_header("Authorization", "Bearer valid-token").match_request(
        has_user_agent,
    ).with_body_text("All conditions matched")

    client_mocker.get("/protected").with_body_text("Fallback response")

    client = ClientBuilder().build()

    success_resp = (
        await client.get("http://api.example.com/protected")
        .header("Authorization", "Bearer valid-token")
        .header("User-Agent", "TestClient/1.0")
        .build_consumed()
        .send()
    )
    assert await success_resp.text() == "All conditions matched"

    no_ua_resp = (
        await client.get("http://api.example.com/protected")
        .header("Authorization", "Bearer valid-token")
        .build_consumed()
        .send()
    )
    assert await no_ua_resp.text() == "Fallback response"

    wrong_auth_resp = (
        await client.get("http://api.example.com/protected")
        .header("Authorization", "Bearer wrong-token")
        .header("User-Agent", "TestClient/1.0")
        .build_consumed()
        .send()
    )
    assert await wrong_auth_resp.text() == "Fallback response"


async def test_custom_handler_basic(client_mocker: ClientMocker) -> None:
    async def echo_handler(request: Request) -> Response | None:
        if request.method == "POST" and "echo" in str(request.url):
            response_builder = (
                ResponseBuilder()
                .status(200)
                .body_json(
                    {
                        "method": request.method,
                        "url": str(request.url),
                        "test_header": request.headers.get("X-Test", "not-found"),
                    },
                )
            )
            return await response_builder.build()
        return None

    client_mocker.mock().match_request_with_response(echo_handler)
    client_mocker.get("/test").with_body_text("Default response")

    client = ClientBuilder().build()

    echo_resp = (
        await client.post("http://api.example.com/echo").header("X-Test", "custom-value").build_consumed().send()
    )

    assert echo_resp.status == 200
    echo_data = await echo_resp.json()
    assert echo_data["method"] == "POST"
    assert echo_data["url"] == "http://api.example.com/echo"
    assert echo_data["test_header"] == "custom-value"

    default_resp = await client.get("http://api.example.com/test").build_consumed().send()
    assert await default_resp.text() == "Default response"


async def test_custom_handler_with_body_inspection(client_mocker: ClientMocker) -> None:
    async def conditional_handler(request: Request) -> Response | None:
        if request.body is None:
            return None

        body_bytes = request.body.copy_bytes()
        if body_bytes is None:
            return None
        body_text = body_bytes.to_bytes().decode()
        body_data = json.loads(body_text)

        if body_data.get("role") == "admin":
            response_builder = (
                ResponseBuilder()
                .status(200)
                .body_json(
                    {
                        "message": f"Admin action: {body_data.get('action', 'unknown')}",
                        "user": body_data.get("user", "anonymous"),
                    },
                )
            )
            return await response_builder.build()

        return None

    mock_cond = client_mocker.mock().match_request_with_response(conditional_handler)
    mock_403 = client_mocker.post("/actions").with_status(403).with_body_text("Forbidden")

    client = ClientBuilder().build()

    admin_resp = (
        await client.post("http://api.example.com/actions")
        .body_text(json.dumps({"role": "admin", "action": "delete", "user": "alice"}))
        .build_consumed()
        .send()
    )

    assert admin_resp.status == 200
    admin_data = await admin_resp.json()
    assert admin_data["message"] == "Admin action: delete"
    assert admin_data["user"] == "alice"

    user_resp = (
        await client.post("http://api.example.com/actions")
        .body_text(json.dumps({"role": "user", "action": "create"}))
        .build_consumed()
        .send()
    )

    assert user_resp.status == 403
    assert await user_resp.text() == "Forbidden"
    assert mock_cond.get_call_count() == 1
    assert mock_403.get_call_count() == 1
    assert client_mocker.get_call_count() == 2


async def test_get_call_count_comprehensive(client_mocker: ClientMocker) -> None:
    users_get_mock = client_mocker.get("/users").with_body_json({"users": []})
    users_post_mock = client_mocker.post("/users").with_status(201).with_body_json({"id": 1})
    posts_get_mock = client_mocker.get("/posts").with_body_json({"posts": []})
    other_put_mock = client_mocker.put("/data").with_status(200).with_body_text("updated")

    client = ClientBuilder().build()

    assert client_mocker.get_call_count() == 0
    assert users_get_mock.get_call_count() == 0
    assert users_post_mock.get_call_count() == 0

    await client.get("http://api.example.com/users").build_consumed().send()
    assert client_mocker.get_call_count() == 1
    assert users_get_mock.get_call_count() == 1
    assert users_post_mock.get_call_count() == 0
    assert posts_get_mock.get_call_count() == 0

    await client.get("http://api.example.com/posts").build_consumed().send()
    assert client_mocker.get_call_count() == 2
    assert users_get_mock.get_call_count() == 1
    assert posts_get_mock.get_call_count() == 1

    await client.get("http://api.example.com/users").build_consumed().send()
    assert client_mocker.get_call_count() == 3
    assert users_get_mock.get_call_count() == 2
    assert posts_get_mock.get_call_count() == 1

    await client.post("http://api.example.com/users").body_text("{}").build_consumed().send()
    assert client_mocker.get_call_count() == 4
    assert users_get_mock.get_call_count() == 2
    assert users_post_mock.get_call_count() == 1

    await client.put("http://other.com/data").body_text("data").build_consumed().send()
    assert client_mocker.get_call_count() == 5
    assert other_put_mock.get_call_count() == 1


async def test_get_call_count_with_custom_handlers(client_mocker: ClientMocker) -> None:
    call_count = 0

    async def custom_handler(request: Request) -> Response | None:
        nonlocal call_count
        if request.method == "GET" and "custom" in str(request.url):
            call_count += 1
            return (
                await ResponseBuilder()
                .status(200)
                .body_text(f"Custom response {call_count}")
                .build()
            )
        return None

    custom_mock = client_mocker.mock().match_request_with_response(custom_handler)
    normal_mock = client_mocker.get("/normal").with_body_text("Normal response")

    client = ClientBuilder().build()

    assert client_mocker.get_call_count() == 0

    resp1 = await client.get("http://api.example.com/custom").build_consumed().send()
    assert await resp1.text() == "Custom response 1"
    assert client_mocker.get_call_count() == 1
    assert custom_mock.get_call_count() == 1

    resp2 = await client.get("http://api.example.com/custom/data").build_consumed().send()
    assert await resp2.text() == "Custom response 2"
    assert client_mocker.get_call_count() == 2
    assert custom_mock.get_call_count() == 2

    resp3 = await client.get("http://api.example.com/normal").build_consumed().send()
    assert await resp3.text() == "Normal response"
    assert client_mocker.get_call_count() == 3
    assert normal_mock.get_call_count() == 1


async def test_get_call_count_after_reset(client_mocker: ClientMocker) -> None:
    get_mock = client_mocker.get("/test").with_body_text("test")
    post_mock = client_mocker.post("/test").with_body_text("posted")

    client = ClientBuilder().build()

    await client.get("http://api.example.com/test").build_consumed().send()
    await client.post("http://api.example.com/test").body_text("data").build_consumed().send()
    await client.get("http://api.example.com/test").build_consumed().send()

    assert client_mocker.get_call_count() == 3
    assert get_mock.get_call_count() == 2
    assert post_mock.get_call_count() == 1

    client_mocker.reset_requests()

    assert client_mocker.get_call_count() == 0
    assert get_mock.get_call_count() == 0
    assert post_mock.get_call_count() == 0
    assert len(client_mocker.get_requests()) == 0


async def test_get_call_count_edge_cases(client_mocker: ClientMocker) -> None:
    mock = client_mocker.strict(True).get("/").with_body_text("response")

    client = ClientBuilder().build()

    assert client_mocker.get_call_count() == 0
    assert mock.get_call_count() == 0

    resp = await client.get("http://test.com").build_consumed().send()
    assert await resp.text() == "response"

    assert client_mocker.get_call_count() == 1
    assert mock.get_call_count() == 1


async def test_query_matching_dict_string_values(client_mocker: ClientMocker) -> None:
    client_mocker.get("/search").match_query({"q": "python", "type": "repo"}).with_body_json({"results": ["pyreqwest"]})
    client_mocker.get("/search").with_body_json({"results": []})

    client = ClientBuilder().build()

    match_resp = await client.get("http://api.example.com/search?q=python&type=repo").build_consumed().send()
    assert await match_resp.json() == {"results": ["pyreqwest"]}

    no_match_resp = await client.get("http://api.example.com/search?q=rust&type=repo").build_consumed().send()
    assert await no_match_resp.json() == {"results": []}

    missing_resp = await client.get("http://api.example.com/search?q=python").build_consumed().send()
    assert await missing_resp.json() == {"results": []}


async def test_query_matching_dict_regex_values(client_mocker: ClientMocker) -> None:
    client_mocker.get("/search").match_query({"q": re.compile(r"py.*"), "limit": re.compile(r"\d+")}).with_body_json(
        {"matched": True},
    )
    client_mocker.get("/search").with_body_json({"matched": False})

    client = ClientBuilder().build()

    match_resp = await client.get("http://api.example.com/search?q=python&limit=10").build_consumed().send()
    assert await match_resp.json() == {"matched": True}

    match2_resp = await client.get("http://api.example.com/search?q=pyreqwest&limit=50").build_consumed().send()
    assert await match2_resp.json() == {"matched": True}

    no_match_resp = await client.get("http://api.example.com/search?q=rust&limit=abc").build_consumed().send()
    assert await no_match_resp.json() == {"matched": False}


async def test_query_matching_regex_pattern(client_mocker: ClientMocker) -> None:
    client_mocker.get("/data").match_query(re.compile(r".*token=\w+.*")).with_body_json({"authorized": True})
    client_mocker.get("/data").with_body_json({"authorized": False})

    client = ClientBuilder().build()

    auth_resp = await client.get("http://api.example.com/data?token=abc123&other=value").build_consumed().send()
    assert await auth_resp.json() == {"authorized": True}

    no_auth_resp = await client.get("http://api.example.com/data?other=value").build_consumed().send()
    assert await no_auth_resp.json() == {"authorized": False}

    empty_resp = await client.get("http://api.example.com/data").build_consumed().send()
    assert await empty_resp.json() == {"authorized": False}


async def test_query_matching_empty(client_mocker: ClientMocker) -> None:
    client_mocker.get("/endpoint").match_query("").with_body_json({"no_params": True})
    client_mocker.get("/endpoint").with_body_json({"has_params": True})

    client = ClientBuilder().build()

    no_params_resp = await client.get("http://api.example.com/endpoint").build_consumed().send()
    assert await no_params_resp.json() == {"no_params": True}

    with_params_resp = await client.get("http://api.example.com/endpoint?foo=bar").build_consumed().send()
    assert await with_params_resp.json() == {"has_params": True}


async def test_query_matching_regex_empty_string(client_mocker: ClientMocker) -> None:
    client_mocker.get("/flexible").match_query(re.compile(r"^$|.*debug=true.*")).with_body_json(
        {"debug_or_empty": True},
    )
    client_mocker.get("/flexible").with_body_json({"other": True})

    client = ClientBuilder().build()

    empty_resp = await client.get("http://api.example.com/flexible").build_consumed().send()
    assert await empty_resp.json() == {"debug_or_empty": True}

    debug_resp = await client.get("http://api.example.com/flexible?debug=true").build_consumed().send()
    assert await debug_resp.json() == {"debug_or_empty": True}

    other_resp = await client.get("http://api.example.com/flexible?other=value").build_consumed().send()
    assert await other_resp.json() == {"other": True}


async def test_query_matching_multiple_values_same_key(client_mocker: ClientMocker) -> None:
    client_mocker.get("/multi").match_query({"tag": ["python", "web"]}).with_body_json({"match": 1})
    client_mocker.get("/multi").match_query({"tag": Contains("rust")}).with_body_json({"match": 2})
    client_mocker.get("/multi").with_body_json({"no_match": True})

    client = ClientBuilder().build()

    resp1 = await client.get("http://api.example.com/multi?tag=python&tag=web").build_consumed().send()
    assert await resp1.json() == {"match": 1}

    resp2 = await client.get("http://api.example.com/multi?tag=python&tag=rust").build_consumed().send()
    assert await resp2.json() == {"match": 2}

    no_match_resp = await client.get("http://api.example.com/multi?tag=python&tag=java").build_consumed().send()
    assert await no_match_resp.json() == {"no_match": True}


async def test_query_matching_mixed_string_and_regex(client_mocker: ClientMocker) -> None:
    client_mocker.get("/mixed").match_query(
        {
            "exact": "value",
            "pattern": re.compile(r"test_\d+"),
            "optional": "",
        },
    ).with_body_json({"mixed_match": True})

    client = ClientBuilder().build()

    match_resp = (
        await client.get("http://api.example.com/mixed?exact=value&pattern=test_123&optional=").build_consumed().send()
    )
    assert await match_resp.json() == {"mixed_match": True}


async def test_query_matching_url_encoded_values(client_mocker: ClientMocker) -> None:
    client_mocker.get("/encoded").match_query({"search": "hello world", "special": "a+b=c"}).with_body_json(
        {"encoded_match": True},
    )

    client = ClientBuilder().build()

    encoded_resp = (
        await client.get("http://api.example.com/encoded?search=hello%20world&special=a%2Bb%3Dc")
        .build_consumed()
        .send()
    )
    assert await encoded_resp.json() == {"encoded_match": True}


async def test_query_matching_case_sensitivity(client_mocker: ClientMocker) -> None:
    client_mocker.get("/case").match_query({"Key": "Value"}).with_body_json({"case_match": True})
    client_mocker.get("/case").with_body_json({"no_match": True})

    client = ClientBuilder().build()

    exact_resp = await client.get("http://api.example.com/case?Key=Value").build_consumed().send()
    assert await exact_resp.json() == {"case_match": True}

    wrong_case_resp = await client.get("http://api.example.com/case?key=value").build_consumed().send()
    assert await wrong_case_resp.json() == {"no_match": True}


async def test_query_matching_with_other_matchers(client_mocker: ClientMocker) -> None:
    client_mocker.post("/combined").match_query({"action": "create"}).match_header(
        "Content-Type",
        "application/json",
    ).match_body(re.compile(r'.*"name":\s*"test".*')).with_body_json({"combined_match": True})

    client_mocker.post("/combined").with_body_json({"partial_match": True})

    client = ClientBuilder().build()

    full_match_resp = (
        await client.post("http://api.example.com/combined?action=create")
        .header("Content-Type", "application/json")
        .body_text('{"name": "test", "other": "data"}')
        .build_consumed()
        .send()
    )
    assert await full_match_resp.json() == {"combined_match": True}

    partial_resp = (
        await client.post("http://api.example.com/combined?action=update")
        .header("Content-Type", "application/json")
        .body_text('{"name": "test", "other": "data"}')
        .build_consumed()
        .send()
    )
    assert await partial_resp.json() == {"partial_match": True}


async def test_query_matching_request_capture(client_mocker: ClientMocker) -> None:
    query_mock = client_mocker.get("/capture").match_query({"filter": "active"}).with_body_json({"captured": True})

    client = ClientBuilder().build()

    await client.get("http://api.example.com/capture?filter=active&sort=name").build_consumed().send()
    await client.get("http://api.example.com/capture?filter=active").build_consumed().send()

    captured_requests = query_mock.get_requests()
    assert len(captured_requests) == 2

    first_request = captured_requests[0]
    assert str(first_request.url) == "http://api.example.com/capture?filter=active&sort=name"
    assert first_request.url.query_dict_multi_value == {"filter": "active", "sort": "name"}

    second_request = captured_requests[1]
    assert str(second_request.url) == "http://api.example.com/capture?filter=active"
    assert second_request.url.query_dict_multi_value == {"filter": "active"}


async def test_json_body_matching_basic(client_mocker: ClientMocker) -> None:
    client_mocker.strict(True).post("/users").match_body_json({"name": "John", "age": 30}).with_status(
        201,
    ).with_body_json({"id": 123})

    client = ClientBuilder().build()

    resp = (
        await client.post("http://api.example.com/users").body_json({"name": "John", "age": 30}).build_consumed().send()
    )
    assert resp.status == 201 and await resp.json() == {"id": 123}

    req = client.post("http://api.example.com/users").body_json({"name": "John", "age": 31}).build_consumed()
    with pytest.raises(AssertionError, match="No mock rule matched request"):
        await req.send()


async def test_json_body_matching_with_custom_equals(client_mocker: ClientMocker) -> None:
    client_mocker.strict(True).post("/partial").match_body_json(
        IsPartialDict(name=IsStr, action="create"),
    ).with_body_text("Partial match successful")

    client = ClientBuilder().build()

    resp1 = (
        await client.post("http://api.example.com/partial")
        .body_json({"name": "Alice", "action": "create", "extra": "ignored"})
        .build_consumed()
        .send()
    )
    assert await resp1.text() == "Partial match successful"

    req = client.post("http://api.example.com/partial").body_json({"action": "create"}).build_consumed()
    with pytest.raises(AssertionError, match="No mock rule matched request"):
        await req.send()


async def test_json_body_matching_invalid(client_mocker: ClientMocker) -> None:
    client_mocker.strict(True).post("/strict").match_body_json({"required": "value"}).with_body_text("Matched")

    client = ClientBuilder().build()

    with pytest.raises(AssertionError, match="No mock rule matched request"):
        await client.post("http://api.example.com/strict").body_text('{"required": value}').build_consumed().send()

    resp = await client.post("http://api.example.com/strict").body_text('{"required":"value"}').build_consumed().send()
    assert await resp.text() == "Matched"
