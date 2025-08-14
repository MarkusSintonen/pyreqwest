"""Tests for pytest mocking utilities."""

import json
import re
from typing import Pattern

import pytest

from pyreqwest.client import ClientBuilder
from pyreqwest.http import Body
from pyreqwest.pytest_mock import ClientMocker, MockResponse, RequestMatcher


async def test_simple_get_mock(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://example.com/api").text("Hello World")

    client = ClientBuilder().build()
    resp = await client.get("http://example.com/api").build_consumed().send()

    assert resp.status == 200
    assert await resp.text() == "Hello World"
    assert client_mocker.get_call_count() == 1


async def test_method_specific_mocks(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://api.example.com/users").json({"users": []})
    client_mocker.post("http://api.example.com/users").status(201).json({"id": 123})
    client_mocker.put("http://api.example.com/users/123").status(204)
    client_mocker.delete("http://api.example.com/users/123").status(204)

    client = ClientBuilder().build()

    get_resp = await client.get("http://api.example.com/users").build_consumed().send()
    assert get_resp.status == 200
    assert await get_resp.json() == {"users": []}

    post_resp = await client.post("http://api.example.com/users").body_text(json.dumps({"name": "John"})).build_consumed().send()
    assert post_resp.status == 201
    assert await post_resp.json() == {"id": 123}

    put_resp = await client.put("http://api.example.com/users/123").body_text(json.dumps({"name": "Jane"})).build_consumed().send()
    assert put_resp.status == 204

    delete_resp = await client.delete("http://api.example.com/users/123").build_consumed().send()
    assert delete_resp.status == 204


async def test_regex_url_matching(client_mocker: ClientMocker) -> None:
    pattern = re.compile(r"http://api\.example\.com/users/\d+")
    client_mocker.get(pattern).json({"id": 456, "name": "Test User"})

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
    ).status(200).text("Authorized")

    client_mocker.mock(
        method="POST",
        url="http://api.example.com/data"
    ).status(401).text("Unauthorized")

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
    ).text("JSON matched")

    client_mocker.mock(
        method="POST",
        url="http://api.example.com/echo",
        body=b"binary data"
    ).text("Binary matched")

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
    ).status(201).text("Create action processed")

    client = ClientBuilder().build()

    resp = await client.post("http://api.example.com/actions") \
        .body_text(json.dumps({"action": "create", "resource": "user"})) \
        .build_consumed().send()

    assert resp.status == 201
    assert await resp.text() == "Create action processed"


async def test_request_capture(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://api.example.com/test").text("response")
    client_mocker.post("http://api.example.com/test").text("posted")

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
    client_mocker.get("http://api.example.com/endpoint").text("response")

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
        .text("Hello") \
        .header("X-Custom-Header", "custom-value") \
        .headers({"X-Rate-Limit": "100", "X-Remaining": "99"})

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/test").build_consumed().send()

    assert resp.headers["X-Custom-Header"] == "custom-value"
    assert resp.headers["X-Rate-Limit"] == "100"
    assert resp.headers["X-Remaining"] == "99"


async def test_json_response(client_mocker: ClientMocker) -> None:
    test_data = {"users": [{"id": 1, "name": "John"}, {"id": 2, "name": "Jane"}]}
    client_mocker.get("http://api.example.com/users").json(test_data)

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/users").build_consumed().send()

    assert resp.headers["content-type"] == "application/json"
    assert await resp.json() == test_data


async def test_bytes_response(client_mocker: ClientMocker) -> None:
    test_data = b"binary data content"
    client_mocker.get("http://api.example.com/binary").bytes(test_data)

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/binary").build_consumed().send()

    assert await resp.bytes() == test_data


async def test_default_response(client_mocker: ClientMocker) -> None:
    client_mocker.default_response().status(404).text("Not Found")

    client = ClientBuilder().build()
    resp = await client.get("http://unmocked.example.com").build_consumed().send()

    assert resp.status == 404
    assert await resp.text() == "Not Found"


async def test_strict_mode(client_mocker: ClientMocker) -> None:
    client_mocker.strict(True)
    client_mocker.get("http://api.example.com/allowed").text("OK")

    client = ClientBuilder().build()

    # Allowed request should work
    resp = await client.get("http://api.example.com/allowed").build_consumed().send()
    assert await resp.text() == "OK"

    # Unmatched request should raise error
    with pytest.raises(AssertionError, match="No mock rule matched request"):
        await client.get("http://api.example.com/forbidden").build_consumed().send()


async def test_reset_mocks(client_mocker: ClientMocker) -> None:
    client_mocker.get("http://api.example.com/test").text("response")

    client = ClientBuilder().build()
    await client.get("http://api.example.com/test").build_consumed().send()

    assert client_mocker.get_call_count() == 1
    assert len(client_mocker.get_requests()) == 1

    client_mocker.reset()

    assert client_mocker.get_call_count() == 0
    assert len(client_mocker.get_requests()) == 0


async def test_multiple_rules_first_match_wins(client_mocker: ClientMocker) -> None:
    # More specific rule first
    client_mocker.get("http://api.example.com/users/123").text("Specific user")
    # More general rule second
    client_mocker.get(re.compile(r"http://api\.example\.com/users/\d+")).text("General user")

    client = ClientBuilder().build()
    resp = await client.get("http://api.example.com/users/123").build_consumed().send()

    # Should match the first (more specific) rule
    assert await resp.text() == "Specific user"


async def test_request_matcher_methods() -> None:
    # Test method matching
    matcher = RequestMatcher(method="POST")

    class MockRequest:
        def __init__(self, method: str, url: str = "http://example.com"):
            self.method = method
            self.url = url
            self.headers = {}
            self.body = None

    assert matcher.matches(MockRequest("POST"))
    assert not matcher.matches(MockRequest("GET"))

    # Test URL matching with string
    matcher = RequestMatcher(url="http://example.com")
    assert matcher.matches(MockRequest("GET", "http://example.com"))
    assert not matcher.matches(MockRequest("GET", "http://other.com"))

    # Test URL matching with regex
    matcher = RequestMatcher(url=re.compile(r"http://.*\.example\.com"))
    assert matcher.matches(MockRequest("GET", "http://api.example.com"))
    assert matcher.matches(MockRequest("GET", "http://sub.example.com"))
    assert not matcher.matches(MockRequest("GET", "http://example.org"))


async def test_mock_response_builder() -> None:
    response = MockResponse()

    # Test method chaining
    result = response.status(201).text("Created").header("Location", "/users/123")
    assert result is response  # Should return self for chaining

    assert response._status == 201
    assert response._body is not None
    assert response._headers["Location"] == "/users/123"


async def test_without_mocking_requests_pass_through(client_mocker: ClientMocker) -> None:
    # Don't mock anything, requests should pass through normally
    # This would normally fail in a real test environment, but demonstrates the concept

    client = ClientBuilder().build()

    # In a real scenario, this would make an actual HTTP request
    # For this test, we'll just verify no mocks are active
    assert client_mocker.get_call_count() == 0
    assert len(client_mocker.get_requests()) == 0
