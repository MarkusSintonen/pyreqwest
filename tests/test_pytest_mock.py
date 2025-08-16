import json
import re
from typing import Any

import pytest

from pyreqwest.client import ClientBuilder
from pyreqwest.pytest_plugin import ClientMocker
from pyreqwest.request import Request
from pyreqwest.response import ResponseBuilder

import_time_client = ClientBuilder().build()


async def test_simple_get_mock(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://example.com/api").with_body_text("Hello World")

    client = ClientBuilder().build()
    resp = await client.get("http://example.com/api").build_consumed().send()

    assert resp.status == 200
    assert await resp.text() == "Hello World"
    assert client_mocker.get_call_count() == 1


async def test_method_specific_mocks(client_mocker: ClientMocker) -> None:
    mock_get = client_mocker.get("http://api.example.com/users").with_body_json({"users": []})
    mock_post = client_mocker.post("http://api.example.com/users").with_status(201).with_body_json({"id": 123})
    mock_put = client_mocker.put("http://api.example.com/users/123").with_status(202)
    mock_delete = client_mocker.delete("http://api.example.com/users/123").with_status(204)

    client = ClientBuilder().build()

    get_resp = await client.get("http://api.example.com/users").build_consumed().send()
    assert get_resp.status == 200
    assert await get_resp.json() == {"users": []}

    post_resp = await client.post("http://api.example.com/users").body_text(json.dumps({"name": "John"})).build_consumed().send()
    assert post_resp.status == 201
    assert await post_resp.json() == {"id": 123}

    put_resp = await client.put("http://api.example.com/users/123").body_text(json.dumps({"name": "Jane"})).build_consumed().send()
    assert put_resp.status == 202

    for _ in range(2):
        delete_resp = await client.delete("http://api.example.com/users/123").build_consumed().send()
        assert delete_resp.status == 204

    assert client_mocker.get_call_count() == 5
    assert mock_get.get_call_count() == 1
    assert mock_post.get_call_count() == 1
    assert mock_put.get_call_count() == 1
    assert mock_delete.get_call_count() == 2


async def test_regex_url_matching(client_mocker: ClientMocker) -> None:
    pattern = re.compile(r"http://api\.example\.com/users/\d+")
    client_mocker.get().match_url(pattern).with_body_json({"id": 456, "name": "Test User"})

    client = ClientBuilder().build()

    resp1 = await client.get("http://api.example.com/users/123").build_consumed().send()
    resp2 = await client.get("http://api.example.com/users/456").build_consumed().send()

    assert await resp1.json() == {"id": 456, "name": "Test User"}
    assert await resp2.json() == {"id": 456, "name": "Test User"}
    assert client_mocker.get_call_count() == 2


async def test_header_matching(client_mocker: ClientMocker) -> None:
    client_mocker.post("http://api.example.com/data") \
        .match_header("Authorization", "Bearer token123") \
        .with_status(200).with_body_text("Authorized")

    client_mocker.post("http://api.example.com/data") \
        .with_status(401).with_body_text("Unauthorized")

    client = ClientBuilder().build()

    # Request with auth header should match first rule
    auth_resp = await client.post("http://api.example.com/data") \
        .header("Authorization", "Bearer token123") \
        .build_consumed().send()
    assert auth_resp.status == 200
    assert await auth_resp.text() == "Authorized"

    # Request without auth header should match second rule
    unauth_resp = await client.post("http://api.example.com/data").build_consumed().send()
    assert unauth_resp.status == 401
    assert await unauth_resp.text() == "Unauthorized"


async def test_body_matching(client_mocker: ClientMocker) -> None:
    client_mocker.post("http://api.example.com/echo") \
        .match_body('{"test": "data"}') \
        .with_body_text("JSON matched")

    client_mocker.post("http://api.example.com/echo") \
        .match_body(b"binary data") \
        .with_body_text("Binary matched")

    client = ClientBuilder().build()

    json_resp = await client.post("http://api.example.com/echo") \
        .body_text('{"test": "data"}') \
        .build_consumed().send()
    assert await json_resp.text() == "JSON matched"

    binary_resp = await client.post("http://api.example.com/echo") \
        .body_bytes(b"binary data") \
        .build_consumed().send()
    assert await binary_resp.text() == "Binary matched"


async def test_regex_body_matching(client_mocker: ClientMocker) -> None:
    pattern = re.compile(r'.*"action":\s*"create".*')
    client_mocker.post("http://api.example.com/actions") \
        .match_body(pattern) \
        .with_status(201).with_body_text("Create action processed")

    client = ClientBuilder().build()

    resp = await client.post("http://api.example.com/actions") \
        .body_text(json.dumps({"action": "create", "resource": "user"})) \
        .build_consumed().send()

    assert resp.status == 201
    assert await resp.text() == "Create action processed"


async def test_request_capture(client_mocker: ClientMocker) -> None:
    get_mock = client_mocker.get("http://api.example.com/test").with_body_text("response")
    post_mock = client_mocker.post("http://api.example.com/test").with_body_text("posted")

    client = ClientBuilder().build()

    await client.get("http://api.example.com/test") \
        .header("User-Agent", "test-client") \
        .build_consumed().send()

    await client.post("http://api.example.com/test") \
        .body_text(json.dumps({"key": "value"})) \
        .build_consumed().send()

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
    mock = client_mocker.get("http://api.example.com/endpoint").with_body_text("response")

    client = ClientBuilder().build()

    for _ in range(3):
        await client.get("http://api.example.com/endpoint").build_consumed().send()

    assert client_mocker.get_call_count() == 3
    assert mock.get_call_count() == 3


async def test_response_headers(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://api.example.com/test") \
        .with_body_text("Hello") \
        .with_header("X-Custom-Header", "custom-value") \
        .with_header("X-Rate-Limit", "100") \
        .with_header("X-Remaining", "99")

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/test").build_consumed().send()

    assert resp.headers["X-Custom-Header"] == "custom-value"
    assert resp.headers["X-Rate-Limit"] == "100"
    assert resp.headers["X-Remaining"] == "99"


async def test_json_response(client_mocker: ClientMocker) -> None:
    test_data = {"users": [{"id": 1, "name": "John"}, {"id": 2, "name": "Jane"}]}
    client_mocker.get("http://api.example.com/users").with_body_json(test_data)

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/users").build_consumed().send()

    assert resp.headers["content-type"] == "application/json"
    assert await resp.json() == test_data


async def test_bytes_response(client_mocker: ClientMocker) -> None:
    test_data = b"binary data content"
    client_mocker.get("http://api.example.com/binary").with_body_bytes(test_data)

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/binary").build_consumed().send()

    assert await resp.bytes() == test_data


async def test_strict_mode(client_mocker: ClientMocker) -> None:
    client_mocker.strict(True)
    client_mocker.get("http://api.example.com/allowed").with_body_text("OK")

    client = ClientBuilder().build()

    resp = await client.get("http://api.example.com/allowed").build_consumed().send()
    assert await resp.text() == "OK"

    with pytest.raises(AssertionError, match="No mock rule matched request"):
        await client.get("http://api.example.com/forbidden").build_consumed().send()


async def test_reset_mocks(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://api.example.com/test").with_body_text("response")

    client = ClientBuilder().build()
    await client.get("http://api.example.com/test").build_consumed().send()

    assert client_mocker.get_call_count() == 1
    assert len(client_mocker.get_requests()) == 1

    client_mocker.reset_requests()

    assert client_mocker.get_call_count() == 0
    assert len(client_mocker.get_requests()) == 0


async def test_multiple_rules_first_match_wins(client_mocker: ClientMocker) -> None:
    # More specific rule first
    client_mocker.get("http://api.example.com/users/123").with_body_text("Specific user")
    # More general rule second
    client_mocker.get().match_url(re.compile(r"http://api\.example\.com/users/\d+")).with_body_text("General user")

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/users/123").build_consumed().send()

    # Should match the first (more specific) rule
    assert await resp.text() == "Specific user"


async def test_method_set_matching(client_mocker: ClientMocker) -> None:
    client_mocker.strict(True)
    client_mocker.mock({"GET", "POST"}, "http://api.example.com/data").with_body_json({"message": "success"})

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


async def test_without_mocking_requests_pass_through(client_mocker: ClientMocker, echo_server) -> None:
    # Mock only specific requests, others should pass through to the real server
    client_mocker.get("http://mocked.example.com/api").with_body_json({"mocked": True, "source": "mock"})

    client = ClientBuilder().build()

    # First request: Should be mocked (matches the mock rule)
    mocked_resp = await client.get("http://mocked.example.com/api").build_consumed().send()
    assert mocked_resp.status == 200
    mocked_data = await mocked_resp.json()
    assert mocked_data["mocked"] is True
    assert mocked_data["source"] == "mock"

    # Second request: Should pass through to real echo server (no matching mock rule)
    real_resp = await client.get(echo_server.url).build_consumed().send()
    assert real_resp.status == 200
    real_data = await real_resp.json()

    # The echo server returns request details, verify we got real data
    assert "method" in real_data
    assert real_data["method"] == "GET"
    assert "headers" in real_data
    assert "body_parts" in real_data
    assert real_data.get("mocked") is None  # Should not have mock data

    # Verify only one mock was triggered (the mocked request)
    assert client_mocker.get_call_count() == 1


async def test_regex_header_matching(client_mocker: ClientMocker) -> None:
    client_mocker.post("http://api.service.com/secure") \
        .match_header("Authorization", re.compile(r"Bearer \w+")) \
        .with_body_json({"authenticated": True})

    client = ClientBuilder().build()

    auth_resp = await client.post("http://api.service.com/secure") \
        .header("Authorization", "Bearer abc123xyz") \
        .build_consumed().send()
    assert (await auth_resp.json())["authenticated"] is True


async def test_mock_chaining_and_reset(client_mocker: ClientMocker) -> None:
    mock = client_mocker.get("http://api.service.com/resource") \
        .with_status(200) \
        .with_body_json({"id": 1, "name": "Resource"}) \
        .with_header("X-Rate-Limit", "100") \
        .with_header("X-Remaining", "99")

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
    async def stream_generator():
        yield b"part1"
        yield b"part2"

    client_mocker.strict(True)
    mock = client_mocker.post("http://api.example.com/stream") \
        .match_body(body_match) \
        .with_body_text("Stream received")

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
    client_mocker.get("http://foo.invalid").with_body_text("Mocked response")

    resp = await import_time_client.get("http://foo.invalid").build_consumed().send()
    assert resp.status == 200
    assert (await resp.text()) == "Mocked response"
    assert client_mocker.get_call_count() == 1


async def test_custom_matcher_basic(client_mocker: ClientMocker) -> None:
    async def has_api_version(request: Request):
        return request.headers.get("X-API-Version") == "v2"

    client_mocker.mock().match_request(has_api_version).with_body_text("API v2 response")
    client_mocker.get().with_body_text("Default response")

    client = ClientBuilder().build()

    v2_resp = await client.get("http://api.example.com/data") \
        .header("X-API-Version", "v2") \
        .build_consumed().send()
    assert await v2_resp.text() == "API v2 response"

    default_resp = await client.get("http://api.example.com/data").build_consumed().send()
    assert await default_resp.text() == "Default response"


async def test_custom_matcher_combined(client_mocker: ClientMocker) -> None:
    """Test custom matcher combined with other standard matchers."""
    async def has_user_agent(request):
        return "TestClient" in request.headers.get("User-Agent", "")

    client_mocker.get("http://api.example.com/protected") \
        .match_header("Authorization", "Bearer valid-token") \
        .match_request(has_user_agent) \
        .with_body_text("All conditions matched")

    client_mocker.get("http://api.example.com/protected").with_body_text("Fallback response")

    client = ClientBuilder().build()

    # Request matching all conditions (method, URL, headers, custom matcher)
    success_resp = await client.get("http://api.example.com/protected") \
        .header("Authorization", "Bearer valid-token") \
        .header("User-Agent", "TestClient/1.0") \
        .build_consumed().send()
    assert await success_resp.text() == "All conditions matched"

    # Request missing User-Agent (custom matcher fails)
    no_ua_resp = await client.get("http://api.example.com/protected") \
        .header("Authorization", "Bearer valid-token") \
        .build_consumed().send()
    assert await no_ua_resp.text() == "Fallback response"

    # Request with wrong auth header (standard matcher fails)
    wrong_auth_resp = await client.get("http://api.example.com/protected") \
        .header("Authorization", "Bearer wrong-token") \
        .header("User-Agent", "TestClient/1.0") \
        .build_consumed().send()
    assert await wrong_auth_resp.text() == "Fallback response"


async def test_custom_handler_basic(client_mocker: ClientMocker) -> None:
    """Test basic custom handler functionality."""
    async def echo_handler(request):
        if request.method == "POST" and "echo" in str(request.url):
            # Create a dynamic response based on request
            response_builder = ResponseBuilder.create_for_mocking() \
                .status(200) \
                .body_json({
                    "method": request.method,
                    "url": str(request.url),
                    "test_header": request.headers.get("X-Test", "not-found"),
                })
            return await response_builder.build()
        return None  # Don't handle this request

    client_mocker.mock().match_request_with_response(echo_handler)
    client_mocker.get("http://api.example.com/test").with_body_text("Default response")

    client = ClientBuilder().build()

    # Request that matches custom handler
    echo_resp = await client.post("http://api.example.com/echo") \
        .header("X-Test", "custom-value") \
        .build_consumed().send()

    assert echo_resp.status == 200
    echo_data = await echo_resp.json()
    assert echo_data["method"] == "POST"
    assert echo_data["url"] == "http://api.example.com/echo"
    assert echo_data["test_header"] == "custom-value"

    # Request that doesn't match custom handler falls back to standard rules
    default_resp = await client.get("http://api.example.com/test").build_consumed().send()
    assert await default_resp.text() == "Default response"


async def test_custom_handler_with_body_inspection(client_mocker: ClientMocker) -> None:
    """Test custom handler that inspects request body."""
    import json

    async def conditional_handler(request):
        if request.body is None:
            return None

        body_bytes = request.body.copy_bytes()
        if body_bytes is None:
            return None
        body_text = body_bytes.to_bytes().decode()
        body_data = json.loads(body_text)

        # Handle admin requests specially
        if body_data.get("role") == "admin":
            response_builder = ResponseBuilder.create_for_mocking() \
                .status(200) \
                .body_json({
                    "message": f"Admin action: {body_data.get('action', 'unknown')}",
                    "user": body_data.get("user", "anonymous")
                })
            return await response_builder.build()

        return None

    mock_cond = client_mocker.mock().match_request_with_response(conditional_handler)
    mock_403 = client_mocker.post("http://api.example.com/actions").with_status(403).with_body_text("Forbidden")

    client = ClientBuilder().build()

    # Admin request handled by custom handler
    admin_resp = await client.post("http://api.example.com/actions") \
        .body_text(json.dumps({"role": "admin", "action": "delete", "user": "alice"})) \
        .build_consumed().send()

    assert admin_resp.status == 200
    admin_data = await admin_resp.json()
    assert admin_data["message"] == "Admin action: delete"
    assert admin_data["user"] == "alice"

    # Regular user request falls back to standard rule
    user_resp = await client.post("http://api.example.com/actions") \
        .body_text(json.dumps({"role": "user", "action": "create"})) \
        .build_consumed().send()

    assert user_resp.status == 403
    assert await user_resp.text() == "Forbidden"
    assert mock_cond.get_call_count() == 1
    assert mock_403.get_call_count() == 1
    assert client_mocker.get_call_count() == 2


async def test_get_call_count_comprehensive(client_mocker: ClientMocker) -> None:
    """Comprehensive test for get_call_count method with various filters and scenarios."""
    # Setup multiple mock rules
    users_get_mock = client_mocker.get("http://api.example.com/users").with_body_json({"users": []})
    users_post_mock = client_mocker.post("http://api.example.com/users").with_status(201).with_body_json({"id": 1})
    posts_get_mock = client_mocker.get("http://api.example.com/posts").with_body_json({"posts": []})
    other_put_mock = client_mocker.put("http://other.com/data").with_status(200).with_body_text("updated")

    client = ClientBuilder().build()

    # Initially no calls
    assert client_mocker.get_call_count() == 0
    assert users_get_mock.get_call_count() == 0
    assert users_post_mock.get_call_count() == 0

    # Make first request
    await client.get("http://api.example.com/users").build_consumed().send()

    assert client_mocker.get_call_count() == 1
    assert users_get_mock.get_call_count() == 1
    assert users_post_mock.get_call_count() == 0
    assert posts_get_mock.get_call_count() == 0

    # Make second request (different endpoint, same method)
    await client.get("http://api.example.com/posts").build_consumed().send()

    assert client_mocker.get_call_count() == 2
    assert users_get_mock.get_call_count() == 1
    assert posts_get_mock.get_call_count() == 1

    # Make third request (same endpoint as first, should increment)
    await client.get("http://api.example.com/users").build_consumed().send()

    assert client_mocker.get_call_count() == 3
    assert users_get_mock.get_call_count() == 2
    assert posts_get_mock.get_call_count() == 1

    # Make POST request
    await client.post("http://api.example.com/users").body_text("{}").build_consumed().send()

    assert client_mocker.get_call_count() == 4
    assert users_get_mock.get_call_count() == 2
    assert users_post_mock.get_call_count() == 1

    # Make PUT request to different domain
    await client.put("http://other.com/data").body_text("data").build_consumed().send()

    assert client_mocker.get_call_count() == 5
    assert other_put_mock.get_call_count() == 1


async def test_get_call_count_with_custom_handlers(client_mocker: ClientMocker) -> None:
    """Test that get_call_count works correctly with custom handlers."""
    call_count = 0

    async def custom_handler(request):
        nonlocal call_count
        if request.method == "GET" and "custom" in str(request.url):
            call_count += 1
            return await ResponseBuilder.create_for_mocking() \
                .status(200) \
                .body_text(f"Custom response {call_count}") \
                .build()
        return None

    custom_mock = client_mocker.mock().match_request_with_response(custom_handler)
    normal_mock = client_mocker.get("http://api.example.com/normal").with_body_text("Normal response")

    client = ClientBuilder().build()

    # Initially no calls
    assert client_mocker.get_call_count() == 0

    # Request handled by custom handler
    resp1 = await client.get("http://api.example.com/custom").build_consumed().send()
    assert await resp1.text() == "Custom response 1"
    assert client_mocker.get_call_count() == 1
    assert custom_mock.get_call_count() == 1

    # Another custom handler request
    resp2 = await client.get("http://api.example.com/custom/data").build_consumed().send()
    assert await resp2.text() == "Custom response 2"
    assert client_mocker.get_call_count() == 2
    assert custom_mock.get_call_count() == 2

    # Request handled by standard mock
    resp3 = await client.get("http://api.example.com/normal").build_consumed().send()
    assert await resp3.text() == "Normal response"
    assert client_mocker.get_call_count() == 3
    assert normal_mock.get_call_count() == 1


async def test_get_call_count_after_reset(client_mocker: ClientMocker) -> None:
    """Test that get_call_count returns 0 after reset."""
    get_mock = client_mocker.get("http://api.example.com/test").with_body_text("test")
    post_mock = client_mocker.post("http://api.example.com/test").with_body_text("posted")

    client = ClientBuilder().build()

    # Make some requests
    await client.get("http://api.example.com/test").build_consumed().send()
    await client.post("http://api.example.com/test").body_text("data").build_consumed().send()
    await client.get("http://api.example.com/test").build_consumed().send()

    # Verify counts before reset
    assert client_mocker.get_call_count() == 3
    assert get_mock.get_call_count() == 2
    assert post_mock.get_call_count() == 1

    # Reset and verify all counts are 0
    client_mocker.reset_requests()

    assert client_mocker.get_call_count() == 0
    assert get_mock.get_call_count() == 0
    assert post_mock.get_call_count() == 0
    assert len(client_mocker.get_requests()) == 0


async def test_get_call_count_edge_cases(client_mocker: ClientMocker) -> None:
    """Test edge cases for get_call_count."""
    mock = client_mocker.get("http://test.com").with_body_text("response")

    client = ClientBuilder().build()

    # Initially no calls
    assert client_mocker.get_call_count() == 0
    assert mock.get_call_count() == 0

    # Make a request
    await client.get("http://test.com").build_consumed().send()

    # Test counts after request
    assert client_mocker.get_call_count() == 1
    assert mock.get_call_count() == 1
