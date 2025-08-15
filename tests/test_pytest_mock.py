"""Tests for pytest mocking utilities."""

import json
import re

import pytest

from pyreqwest.client import ClientBuilder
from pyreqwest.pytest_plugin import ClientMocker, RequestMatcher
from pyreqwest.request import Request, RequestBuilder
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

    # Get all requests
    all_requests = client_mocker.get_requests()
    assert len(all_requests) == 2

    # Get filtered requests
    get_requests = client_mocker.get_requests(method="GET")
    assert len(get_requests) == 1
    assert get_requests[0].method == "GET"
    assert get_requests[0].headers.get("User-Agent") == "test-client"

    post_requests = client_mocker.get_requests(method="POST")
    assert len(post_requests) == 1
    assert post_requests[0].method == "POST"

    # Get requests by URL
    api_requests = client_mocker.get_requests(url="http://api.example.com/test")
    assert len(api_requests) == 2


async def test_call_counting(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://api.example.com/endpoint").body_text("response")

    client = ClientBuilder().build()

    # Make multiple calls
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

    # Allowed request should work
    resp = await client.get("http://api.example.com/allowed").build_consumed().send()
    assert await resp.text() == "OK"

    # Unmatched request should raise error
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


async def test_request_matcher_methods() -> None:
    # Test method matching
    matcher = RequestMatcher(method="POST")
    client = ClientBuilder().build()

    def mock_request(method: str, url: str = "http://example.com") -> Request:
        return client.request(method, url).build_consumed()

    assert matcher.matches(mock_request("POST"))
    assert not matcher.matches(mock_request("GET"))

    # Test URL matching with string
    matcher = RequestMatcher(url="http://example.com")
    assert matcher.matches(mock_request("GET", "http://example.com"))
    assert not matcher.matches(mock_request("GET", "http://other.com"))

    # Test URL matching with regex
    matcher = RequestMatcher(url=re.compile(r"http://.*\.example\.com"))
    assert matcher.matches(mock_request("GET", "http://api.example.com"))
    assert matcher.matches(mock_request("GET", "http://sub.example.com"))
    assert not matcher.matches(mock_request("GET", "http://example.org"))


async def test_mock_response_builder() -> None:
    response = ResponseBuilder.create_for_mocking()

    # Test method chaining
    result = response.status(201).header("Location", "/users/123")
    assert result is response  # Should return self for chaining

    # Since ResponseBuilder is from Rust, we can't access internal state directly
    # Instead, we test that it builds a proper response
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
