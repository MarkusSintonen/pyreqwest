import json
import re
from typing import Any

import pytest

from pyreqwest.client import ClientBuilder
from pyreqwest.pytest_plugin import ClientMocker
from pyreqwest.request import Request
from pyreqwest.response import ResponseBuilder


async def test_simple_get_mock(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://example.com/api").body_text("Hello World")

    client = ClientBuilder().build()
    resp = await client.get("http://example.com/api").build_consumed().send()

    assert resp.status == 200
    assert await resp.text() == "Hello World"
    assert client_mocker.get_call_count() == 1


async def test_method_specific_mocks(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://api.example.com/users").body_json({"users": []})
    client_mocker.post("http://api.example.com/users").status(201).body_json({"id": 123})
    client_mocker.put("http://api.example.com/users/123").status(202)
    client_mocker.delete("http://api.example.com/users/123").status(204)

    client = ClientBuilder().build()

    get_resp = await client.get("http://api.example.com/users").build_consumed().send()
    assert get_resp.status == 200
    assert await get_resp.json() == {"users": []}

    post_resp = await client.post("http://api.example.com/users").body_text(json.dumps({"name": "John"})).build_consumed().send()
    assert post_resp.status == 201
    assert await post_resp.json() == {"id": 123}

    put_resp = await client.put("http://api.example.com/users/123").body_text(json.dumps({"name": "Jane"})).build_consumed().send()
    assert put_resp.status == 202

    delete_resp = await client.delete("http://api.example.com/users/123").build_consumed().send()
    assert delete_resp.status == 204


async def test_regex_url_matching(client_mocker: ClientMocker) -> None:
    pattern = re.compile(r"http://api\.example\.com/users/\d+")
    client_mocker.get(pattern).body_json({"id": 456, "name": "Test User"})

    client = ClientBuilder().build()

    resp1 = await client.get("http://api.example.com/users/123").build_consumed().send()
    resp2 = await client.get("http://api.example.com/users/456").build_consumed().send()

    assert await resp1.json() == {"id": 456, "name": "Test User"}
    assert await resp2.json() == {"id": 456, "name": "Test User"}
    assert client_mocker.get_call_count() == 2


async def test_header_matching(client_mocker: ClientMocker) -> None:
    client_mocker.mock(
        method="POST",
        url="http://api.example.com/data",
        headers={"Authorization": "Bearer token123"}
    ).status(200).body_text("Authorized")

    client_mocker.mock(
        method="POST",
        url="http://api.example.com/data"
    ).status(401).body_text("Unauthorized")

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
    client_mocker.mock(
        method="POST",
        url="http://api.example.com/echo",
        body='{"test": "data"}'
    ).body_text("JSON matched")

    client_mocker.mock(
        method="POST",
        url="http://api.example.com/echo",
        body=b"binary data"
    ).body_text("Binary matched")

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
    client_mocker.mock(
        method="POST",
        url="http://api.example.com/actions",
        body=pattern
    ).status(201).body_text("Create action processed")

    client = ClientBuilder().build()

    resp = await client.post("http://api.example.com/actions") \
        .body_text(json.dumps({"action": "create", "resource": "user"})) \
        .build_consumed().send()

    assert resp.status == 201
    assert await resp.text() == "Create action processed"


async def test_request_capture(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://api.example.com/test").body_text("response")
    client_mocker.post("http://api.example.com/test").body_text("posted")

    client = ClientBuilder().build()

    await client.get("http://api.example.com/test") \
        .header("User-Agent", "test-client") \
        .build_consumed().send()

    await client.post("http://api.example.com/test") \
        .body_text(json.dumps({"key": "value"})) \
        .build_consumed().send()

    all_requests = client_mocker.get_requests()
    assert len(all_requests) == 2

    get_requests = client_mocker.get_requests(method="GET")
    assert len(get_requests) == 1
    assert get_requests[0].method == "GET"
    assert get_requests[0].headers.get("User-Agent") == "test-client"

    post_requests = client_mocker.get_requests(method="POST")
    assert len(post_requests) == 1
    assert post_requests[0].method == "POST"

    api_requests = client_mocker.get_requests(url="http://api.example.com/test")
    assert len(api_requests) == 2


async def test_call_counting(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://api.example.com/endpoint").body_text("response")

    client = ClientBuilder().build()

    for _ in range(3):
        await client.get("http://api.example.com/endpoint").build_consumed().send()

    assert client_mocker.get_call_count() == 3
    assert client_mocker.get_call_count(method="GET") == 3
    assert client_mocker.get_call_count(method="POST") == 0
    assert client_mocker.get_call_count(url="http://api.example.com/endpoint") == 3


async def test_response_headers(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://api.example.com/test") \
        .body_text("Hello") \
        .header("X-Custom-Header", "custom-value") \
        .headers({"X-Rate-Limit": "100", "X-Remaining": "99"})

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/test").build_consumed().send()

    assert resp.headers["X-Custom-Header"] == "custom-value"
    assert resp.headers["X-Rate-Limit"] == "100"
    assert resp.headers["X-Remaining"] == "99"


async def test_json_response(client_mocker: ClientMocker) -> None:
    test_data = {"users": [{"id": 1, "name": "John"}, {"id": 2, "name": "Jane"}]}
    client_mocker.get("http://api.example.com/users").body_json(test_data)

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/users").build_consumed().send()

    assert resp.headers["content-type"] == "application/json"
    assert await resp.json() == test_data


async def test_bytes_response(client_mocker: ClientMocker) -> None:
    test_data = b"binary data content"
    client_mocker.get("http://api.example.com/binary").body_bytes(test_data)

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/binary").build_consumed().send()

    assert await resp.bytes() == test_data


async def test_strict_mode(client_mocker: ClientMocker) -> None:
    client_mocker.strict(True)
    client_mocker.get("http://api.example.com/allowed").body_text("OK")

    client = ClientBuilder().build()

    resp = await client.get("http://api.example.com/allowed").build_consumed().send()
    assert await resp.text() == "OK"

    with pytest.raises(AssertionError, match="No mock rule matched request"):
        await client.get("http://api.example.com/forbidden").build_consumed().send()


async def test_reset_mocks(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://api.example.com/test").body_text("response")

    client = ClientBuilder().build()
    await client.get("http://api.example.com/test").build_consumed().send()

    assert client_mocker.get_call_count() == 1
    assert len(client_mocker.get_requests()) == 1

    client_mocker.reset()

    assert client_mocker.get_call_count() == 0
    assert len(client_mocker.get_requests()) == 0


async def test_multiple_rules_first_match_wins(client_mocker: ClientMocker) -> None:
    # More specific rule first
    client_mocker.get("http://api.example.com/users/123").body_text("Specific user")
    # More general rule second
    client_mocker.get(re.compile(r"http://api\.example\.com/users/\d+")).body_text("General user")

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/users/123").build_consumed().send()

    # Should match the first (more specific) rule
    assert await resp.text() == "Specific user"


async def test_method_set_matching(client_mocker: ClientMocker) -> None:
    client_mocker.strict(True).mock(method={"GET", "POST"}, url="http://api.example.com/data").body_json({"message": "success"})

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
    requests = client_mocker.get_requests()
    assert len(requests) == 2
    assert requests[0].method == "GET"
    assert requests[1].method == "POST"


async def test_mock_response_builder() -> None:
    response = ResponseBuilder.create_for_mocking()

    result = response.status(201).header("Location", "/users/123")
    assert result is response

    built_response = await response.build()
    assert built_response.status == 201
    assert built_response.headers["Location"] == "/users/123"


async def test_without_mocking_requests_pass_through(client_mocker: ClientMocker, echo_server) -> None:
    # Mock only specific requests, others should pass through to the real server
    client_mocker.get("http://mocked.example.com/api").body_json({"mocked": True, "source": "mock"})

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
    captured_requests = client_mocker.get_requests()
    assert len(captured_requests) == 1
    assert str(captured_requests[0].url) == "http://mocked.example.com/api"


async def test_regex_header_matching(client_mocker: ClientMocker) -> None:
    client_mocker.mock(
        method="POST",
        url="http://api.service.com/secure",
        headers={"Authorization": re.compile(r"Bearer \w+")}
    ).body_json({"authenticated": True})

    client = ClientBuilder().build()

    auth_resp = await client.post("http://api.service.com/secure") \
        .header("Authorization", "Bearer abc123xyz") \
        .build_consumed().send()
    assert (await auth_resp.json())["authenticated"] is True


async def test_mock_chaining_and_reset(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://api.service.com/resource") \
        .status(200) \
        .body_json({"id": 1, "name": "Resource"}) \
        .header("X-Rate-Limit", "100") \
        .header("X-Remaining", "99")

    client = ClientBuilder().build()

    resp = await client.get("http://api.service.com/resource").build_consumed().send()
    assert resp.status == 200
    assert resp.headers["X-Rate-Limit"] == "100"
    assert (await resp.json())["name"] == "Resource"

    assert client_mocker.get_call_count() == 1

    client_mocker.reset()
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

    client_mocker.strict(True).mock(
        method="POST", url="http://api.example.com/stream", body=body_match
    ).body_text("Stream received")

    client = ClientBuilder().error_for_status(True).build()
    req = client.post("http://api.example.com/stream").body_stream(stream_generator()).build_consumed()

    if matches:
        resp = await req.send()
        assert await resp.text() == "Stream received"
        assert len(client_mocker.get_requests()) == 1
        request = client_mocker.get_requests()[0]
        assert request.method == "POST"
        assert request.url == "http://api.example.com/stream"
        assert request.body is not None and request.body.copy_bytes() == b"part1part2"
    else:
        with pytest.raises(AssertionError, match="No mock rule matched request"):
            await req.send()
        assert len(client_mocker.get_requests()) == 0


import_time_client = ClientBuilder().build()


async def test_import_time_client_is_mocked(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://foo.invalid").body_text("Mocked response")

    resp = await import_time_client.get("http://foo.invalid").build_consumed().send()
    assert resp.status == 200
    assert (await resp.text()) == "Mocked response"
    assert client_mocker.get_call_count() == 1


async def test_custom_matcher_basic(client_mocker: ClientMocker) -> None:
    def has_api_version(request: Request):
        return request.headers.get("X-API-Version") == "v2"

    client_mocker.mock(custom_matcher=has_api_version).body_text("API v2 response")
    client_mocker.get().body_text("Default response")

    client = ClientBuilder().build()

    v2_resp = await client.get("http://api.example.com/data") \
        .header("X-API-Version", "v2") \
        .build_consumed().send()
    assert await v2_resp.text() == "API v2 response"

    default_resp = await client.get("http://api.example.com/data").build_consumed().send()
    assert await default_resp.text() == "Default response"


async def test_custom_matcher_combined(client_mocker: ClientMocker) -> None:
    """Test custom matcher combined with other standard matchers."""
    def has_user_agent(request):
        return "TestClient" in request.headers.get("User-Agent", "")

    client_mocker.mock(
        method="GET",
        url="http://api.example.com/protected",
        headers={"Authorization": "Bearer valid-token"},
        custom_matcher=has_user_agent
    ).body_text("All conditions matched")

    client_mocker.get("http://api.example.com/protected").body_text("Fallback response")

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

    client_mocker.custom_handler(echo_handler)
    client_mocker.get("http://api.example.com/test").body_text("Default response")

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

        try:
            body_bytes = request.body.copy_bytes()
            if body_bytes is None:
                return None
            body_text = bytes(body_bytes).decode()
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

        except (json.JSONDecodeError, AttributeError):
            pass

        return None

    client_mocker.custom_handler(conditional_handler)
    client_mocker.post("http://api.example.com/actions").status(403).body_text("Forbidden")

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


async def test_get_call_count_comprehensive(client_mocker: ClientMocker) -> None:
    """Comprehensive test for get_call_count method with various filters and scenarios."""
    # Setup multiple mock rules
    client_mocker.get("http://api.example.com/users").body_json({"users": []})
    client_mocker.post("http://api.example.com/users").status(201).body_json({"id": 1})
    client_mocker.get("http://api.example.com/posts").body_json({"posts": []})
    client_mocker.put("http://other.com/data").status(200).body_text("updated")

    client = ClientBuilder().build()

    # Initially no calls
    assert client_mocker.get_call_count() == 0
    assert client_mocker.get_call_count(method="GET") == 0
    assert client_mocker.get_call_count(method="POST") == 0
    assert client_mocker.get_call_count(url="http://api.example.com/users") == 0

    # Make first request
    await client.get("http://api.example.com/users").build_consumed().send()

    assert client_mocker.get_call_count() == 1
    assert client_mocker.get_call_count(method="GET") == 1
    assert client_mocker.get_call_count(method="POST") == 0
    assert client_mocker.get_call_count(method="PUT") == 0
    assert client_mocker.get_call_count(url="http://api.example.com/users") == 1
    assert client_mocker.get_call_count(url="http://api.example.com/posts") == 0

    # Make second request (different endpoint, same method)
    await client.get("http://api.example.com/posts").build_consumed().send()

    assert client_mocker.get_call_count() == 2
    assert client_mocker.get_call_count(method="GET") == 2
    assert client_mocker.get_call_count(method="POST") == 0
    assert client_mocker.get_call_count(url="http://api.example.com/users") == 1
    assert client_mocker.get_call_count(url="http://api.example.com/posts") == 1

    # Make third request (same endpoint as first, should increment)
    await client.get("http://api.example.com/users").build_consumed().send()

    assert client_mocker.get_call_count() == 3
    assert client_mocker.get_call_count(method="GET") == 3
    assert client_mocker.get_call_count(url="http://api.example.com/users") == 2
    assert client_mocker.get_call_count(url="http://api.example.com/posts") == 1

    # Make POST request
    await client.post("http://api.example.com/users").body_text("{}").build_consumed().send()

    assert client_mocker.get_call_count() == 4
    assert client_mocker.get_call_count(method="GET") == 3
    assert client_mocker.get_call_count(method="POST") == 1
    assert client_mocker.get_call_count(url="http://api.example.com/users") == 3  # Both GET and POST

    # Make PUT request to different domain
    await client.put("http://other.com/data").body_text("data").build_consumed().send()

    assert client_mocker.get_call_count() == 5
    assert client_mocker.get_call_count(method="GET") == 3
    assert client_mocker.get_call_count(method="POST") == 1
    assert client_mocker.get_call_count(method="PUT") == 1
    assert client_mocker.get_call_count(url="http://other.com/data") == 1
    assert client_mocker.get_call_count(url="http://api.example.com/users") == 3

    # Test with regex URL matching
    import re
    pattern = re.compile(r"http://api\.example\.com/.*")
    assert client_mocker.get_call_count(url=pattern) == 4  # All api.example.com requests

    # Test non-existent method/url
    assert client_mocker.get_call_count(method="DELETE") == 0
    assert client_mocker.get_call_count(url="http://nonexistent.com") == 0

    # Test combined filters (method AND url)
    assert client_mocker.get_call_count(method="GET", url="http://api.example.com/users") == 2
    assert client_mocker.get_call_count(method="POST", url="http://api.example.com/users") == 1
    assert client_mocker.get_call_count(method="GET", url="http://api.example.com/posts") == 1
    assert client_mocker.get_call_count(method="POST", url="http://api.example.com/posts") == 0


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

    client_mocker.custom_handler(custom_handler)
    client_mocker.get("http://api.example.com/normal").body_text("Normal response")

    client = ClientBuilder().build()

    # Initially no calls
    assert client_mocker.get_call_count() == 0

    # Request handled by custom handler
    resp1 = await client.get("http://api.example.com/custom").build_consumed().send()
    assert await resp1.text() == "Custom response 1"
    assert client_mocker.get_call_count() == 1
    assert client_mocker.get_call_count(method="GET") == 1

    # Another custom handler request
    resp2 = await client.get("http://api.example.com/custom/data").build_consumed().send()
    assert await resp2.text() == "Custom response 2"
    assert client_mocker.get_call_count() == 2
    assert client_mocker.get_call_count(method="GET") == 2

    # Request handled by standard mock
    resp3 = await client.get("http://api.example.com/normal").build_consumed().send()
    assert await resp3.text() == "Normal response"
    assert client_mocker.get_call_count() == 3
    assert client_mocker.get_call_count(method="GET") == 3

    # Test URL filtering with custom handler requests
    custom_pattern = re.compile(r".*custom.*")
    assert client_mocker.get_call_count(url=custom_pattern) == 2
    assert client_mocker.get_call_count(url="http://api.example.com/normal") == 1


async def test_get_call_count_after_reset(client_mocker: ClientMocker) -> None:
    """Test that get_call_count returns 0 after reset."""
    client_mocker.get("http://api.example.com/test").body_text("test")
    client_mocker.post("http://api.example.com/test").body_text("posted")

    client = ClientBuilder().build()

    # Make some requests
    await client.get("http://api.example.com/test").build_consumed().send()
    await client.post("http://api.example.com/test").body_text("data").build_consumed().send()
    await client.get("http://api.example.com/test").build_consumed().send()

    # Verify counts before reset
    assert client_mocker.get_call_count() == 3
    assert client_mocker.get_call_count(method="GET") == 2
    assert client_mocker.get_call_count(method="POST") == 1
    assert client_mocker.get_call_count(url="http://api.example.com/test") == 3

    # Reset and verify all counts are 0
    client_mocker.reset()

    assert client_mocker.get_call_count() == 0
    assert client_mocker.get_call_count(method="GET") == 0
    assert client_mocker.get_call_count(method="POST") == 0
    assert client_mocker.get_call_count(method="PUT") == 0
    assert client_mocker.get_call_count(url="http://api.example.com/test") == 0
    assert client_mocker.get_call_count(url="http://any.url.com") == 0


async def test_get_call_count_edge_cases(client_mocker: ClientMocker) -> None:
    """Test edge cases for get_call_count."""
    client_mocker.get("http://test.com").body_text("response")

    client = ClientBuilder().build()

    # Test with empty string parameters (should be treated as None)
    assert client_mocker.get_call_count(method="") == 0
    assert client_mocker.get_call_count(url="") == 0

    # Test with None parameters explicitly
    assert client_mocker.get_call_count(method=None) == 0
    assert client_mocker.get_call_count(url=None) == 0
    assert client_mocker.get_call_count(method=None, url=None) == 0

    # Make a request
    await client.get("http://test.com").build_consumed().send()

    # Test that None parameters return total count
    assert client_mocker.get_call_count(method=None) == 1
    assert client_mocker.get_call_count(url=None) == 1
    assert client_mocker.get_call_count(method=None, url=None) == 1

    # Test case sensitivity for methods
    assert client_mocker.get_call_count(method="get") == 0  # Should be 0, methods are case sensitive
    assert client_mocker.get_call_count(method="GET") == 1


async def test_custom_handler_fallback_to_standard_matchers(client_mocker: ClientMocker) -> None:
    """Test that when custom handler returns None, it falls back to standard matchers for the same rule."""
    async def selective_handler(request):
        # Only handle POST requests, return None for others
        if request.method == "POST":
            return await ResponseBuilder.create_for_mocking() \
                .status(201) \
                .body_text("Custom POST response") \
                .build()
        return None  # Let standard matchers handle this

    # Create a rule with both custom handler AND standard matchers
    client_mocker.mock(
        method="GET",
        url="http://api.example.com/data",
        custom_handler=selective_handler
    ).status(200).body_text("Standard GET response")

    # Also add a fallback rule
    client_mocker.get("http://api.example.com/other").body_text("Other response")

    client = ClientBuilder().build()

    # POST request should be handled by custom handler
    post_resp = await client.post("http://api.example.com/data").build_consumed().send()
    assert post_resp.status == 201
    assert await post_resp.text() == "Custom POST response"

    # GET request should fall back to standard matchers for the same rule
    get_resp = await client.get("http://api.example.com/data").build_consumed().send()
    assert get_resp.status == 200
    assert await get_resp.text() == "Standard GET response"

    # Different URL should use different rule
    other_resp = await client.get("http://api.example.com/other").build_consumed().send()
    assert await other_resp.text() == "Other response"

    # Verify call counts
    assert client_mocker.get_call_count() == 3
    assert client_mocker.get_call_count(method="POST") == 1
    assert client_mocker.get_call_count(method="GET") == 2


async def test_custom_handler_with_standard_matchers_combined(client_mocker: ClientMocker) -> None:
    """Test custom handler combined with standard matchers on the same rule."""
    async def auth_required_handler(request):
        # Only handle requests with auth header
        if request.headers.get("Authorization"):
            return await ResponseBuilder.create_for_mocking() \
                .status(200) \
                .body_json({"authenticated": True, "message": "Custom auth response"}) \
                .build()
        return None  # No auth header, let standard matchers handle

    # Rule with custom handler AND standard matchers (method + URL)
    client_mocker.mock(
        method="POST",
        url="http://api.example.com/secure",
        custom_handler=auth_required_handler
    ).status(401).body_json({"error": "Unauthorized"})

    client = ClientBuilder().build()

    # Request with auth header should be handled by custom handler
    auth_resp = await client.post("http://api.example.com/secure") \
        .header("Authorization", "Bearer token") \
        .build_consumed().send()

    assert auth_resp.status == 200
    auth_data = await auth_resp.json()
    assert auth_data["authenticated"] is True
    assert auth_data["message"] == "Custom auth response"

    # Request without auth header should fall back to standard matchers
    unauth_resp = await client.post("http://api.example.com/secure").build_consumed().send()

    assert unauth_resp.status == 401
    unauth_data = await unauth_resp.json()
    assert unauth_data["error"] == "Unauthorized"

    # Wrong method shouldn't match the rule at all (since standard matchers check method)
    client_mocker.get("http://fallback.com").body_text("Fallback")

    get_resp = await client.get("http://api.example.com/secure").build_consumed().send()
    assert await get_resp.text() == "Fallback"

    assert client_mocker.get_call_count() == 3
