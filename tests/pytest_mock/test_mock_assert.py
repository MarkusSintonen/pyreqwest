import json
import re

import pytest
from syrupy import SnapshotAssertion

from pyreqwest.client import ClientBuilder, Client
from pyreqwest.pytest_plugin import ClientMocker
from pyreqwest.request import Request
from pyreqwest.response import ResponseBuilder, Response


@pytest.fixture
def client() -> Client:
    return ClientBuilder().build()


async def test_assert_called_default_exactly_once_success(client_mocker: ClientMocker, client: Client) -> None:
    mock = client_mocker.get("http://api.example.com/test").with_body_text("response")

    await client.get("http://api.example.com/test").build_consumed().send()

    mock.assert_called()


async def test_assert_called_default_exactly_once_failure(client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion) -> None:
    mock = client_mocker.get("http://api.example.com/test").with_body_text("response")
    client_mocker.get("http://api.example.com/different").with_body_text("different response")

    await client.get("http://api.example.com/different").build_consumed().send()

    with pytest.raises(AssertionError) as exc_info:
        mock.assert_called()

    assert str(exc_info.value) == snapshot


async def test_assert_called_exact_count_success(client_mocker: ClientMocker, client: Client) -> None:
    mock = client_mocker.get("http://api.example.com/test").with_body_text("response")

    for _ in range(3):
        await client.get("http://api.example.com/test").build_consumed().send()

    mock.assert_called(count=3)


async def test_assert_called_exact_count_failure(client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion) -> None:
    mock = client_mocker.post("http://api.example.com/users") \
        .match_header("Authorization", "Bearer token123") \
        .match_body_json({"name": "John", "age": 30}) \
        .with_status(201) \
        .with_body_json({"id": 1})

    client_mocker.post("http://api.example.com/users").with_status(403).with_body_text("Forbidden")
    client_mocker.get("http://api.example.com/users").with_body_json({"users": []})

    res = await client.post("http://api.example.com/users") \
        .header("Authorization", "Bearer token123") \
        .body_json({"name": "John", "age": 30}) \
        .build_consumed().send()
    assert await res.json() == {"id": 1}

    res = await client.post("http://api.example.com/users") \
        .header("Authorization", "Bearer wrong-token") \
        .body_json({"name": "Jane", "age": 25}) \
        .build_consumed().send()
    assert res.status == 403

    res = await client.get("http://api.example.com/users").build_consumed().send()
    assert await res.json() == {"users": []}

    with pytest.raises(AssertionError) as exc_info:
        mock.assert_called(count=3)

    assert str(exc_info.value) == snapshot


async def test_assert_called_min_count_success(client_mocker: ClientMocker, client: Client) -> None:
    mock = client_mocker.get("http://api.example.com/test").with_body_text("response")

    for _ in range(5):
        await client.get("http://api.example.com/test").build_consumed().send()

    mock.assert_called(min_count=3)


async def test_assert_called_min_count_failure(client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion) -> None:
    mock = client_mocker.get("http://api.example.com/endpoint") \
        .match_query({"filter": "active"}) \
        .with_body_json({"data": []})

    client_mocker.get("http://api.example.com/endpoint").with_body_json({"data": ["inactive"]})

    await client.get("http://api.example.com/endpoint?filter=active").build_consumed().send()
    await client.get("http://api.example.com/endpoint?filter=inactive").build_consumed().send()

    with pytest.raises(AssertionError) as exc_info:
        mock.assert_called(min_count=3)

    assert str(exc_info.value) == snapshot


async def test_assert_called_max_count_success(client_mocker: ClientMocker, client: Client) -> None:
    mock = client_mocker.get("http://api.example.com/test").with_body_text("response")

    for _ in range(2):
        await client.get("http://api.example.com/test").build_consumed().send()

    mock.assert_called(max_count=3)


async def test_assert_called_max_count_failure(client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion) -> None:
    mock = client_mocker.get("http://api.example.com/test").with_body_text("response")

    for _ in range(5):
        await client.get("http://api.example.com/test").build_consumed().send()

    with pytest.raises(AssertionError) as exc_info:
        mock.assert_called(max_count=3)

    assert str(exc_info.value) == snapshot


async def test_assert_called_min_max_range_success(client_mocker: ClientMocker, client: Client) -> None:
    mock = client_mocker.get("http://api.example.com/test").with_body_text("response")

    for _ in range(3):
        await client.get("http://api.example.com/test").build_consumed().send()

    mock.assert_called(min_count=2, max_count=5)


async def test_assert_called_min_max_range_failure(client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion) -> None:
    mock = client_mocker.get("http://api.example.com/test").with_body_text("response")

    await client.get("http://api.example.com/test").build_consumed().send()

    with pytest.raises(AssertionError) as exc_info:
        mock.assert_called(min_count=3, max_count=5)

    assert str(exc_info.value) == snapshot


async def test_assert_called_complex_mock_with_all_matchers(client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion) -> None:
    mock = client_mocker.post("http://api.example.com/complex") \
        .match_header("Authorization", re.compile(r"Bearer \w+")) \
        .match_header("Content-Type", "application/json") \
        .match_query({"action": "create", "version": re.compile(r"v\d+")}) \
        .match_body_json({"user": {"name": "John", "role": "admin"}}) \
        .with_status(201)

    client_mocker.get("http://api.example.com/complex").with_status(405).with_body_text("Method not allowed")
    client_mocker.post("http://api.example.com/complex").with_status(400).with_body_text("Bad request")

    requests_to_make = [
        # Wrong method
        ("GET", "http://api.example.com/complex?action=create&version=v1", {}, None),
        # Missing auth header
        ("POST", "http://api.example.com/complex?action=create&version=v1", {"Content-Type": "application/json"}, {"user": {"name": "John", "role": "admin"}}),
        # Wrong query param
        ("POST", "http://api.example.com/complex?action=update&version=v1", {"Authorization": "Bearer abc123", "Content-Type": "application/json"}, {"user": {"name": "John", "role": "admin"}}),
        # Wrong body
        ("POST", "http://api.example.com/complex?action=create&version=v1", {"Authorization": "Bearer abc123", "Content-Type": "application/json"}, {"user": {"name": "Jane", "role": "user"}}),
    ]

    for method, url, headers, body in requests_to_make:
        req_builder = client.request(method, url)
        for header_name, header_value in headers.items():
            req_builder = req_builder.header(header_name, header_value)
        if body is not None:
            req_builder = req_builder.body_json(body)
        await req_builder.build_consumed().send()

    with pytest.raises(AssertionError) as exc_info:
        mock.assert_called()

    assert str(exc_info.value) == snapshot


async def test_assert_called_custom_matcher_and_handler(client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion) -> None:
    async def is_admin_request(request: Request) -> bool:
        if request.body is None or (body_bytes := request.body.copy_bytes()) is None:
            return False
        return json.loads(body_bytes.to_bytes()).get("role") == "admin"

    async def admin_handler(_request: Request) -> Response:
        return await ResponseBuilder.create_for_mocking() \
            .status(200) \
            .body_json({"message": "Admin access granted"}) \
            .build()

    mock = client_mocker.post("http://api.example.com/admin") \
        .match_request(is_admin_request) \
        .match_request_with_response(admin_handler)

    client_mocker.post("http://api.example.com/admin").with_status(403).with_body_text("Forbidden")

    res = await client.post("http://api.example.com/admin") \
        .body_json({"role": "user", "action": "view"}) \
        .build_consumed().send()
    assert await res.text() == "Forbidden"

    res = await client.post("http://api.example.com/admin") \
        .body_json({"role": "admin", "action": "view"}) \
        .build_consumed().send()
    assert await res.json() == {"message": "Admin access granted"}

    with pytest.raises(AssertionError) as exc_info:
        mock.assert_called(count=3)

    assert str(exc_info.value) == snapshot


async def test_assert_called_with_matched_and_unmatched_requests(client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion) -> None:
    mock = client_mocker.get("http://api.example.com/users") \
        .match_query({"active": "true"}) \
        .with_body_json({"users": []})

    client_mocker.get("http://api.example.com/users").with_body_json({"users": ["inactive"]})
    client_mocker.get("http://api.example.com/posts").with_body_json({"posts": []})

    for i in range(2):
        await client.get(f"http://api.example.com/users?active=true&page={i}").build_consumed().send()

    unmatched_requests = [
        "http://api.example.com/users?active=false",
        "http://api.example.com/users",
        "http://api.example.com/posts?active=true",
    ]

    for url in unmatched_requests:
        await client.get(url).build_consumed().send()

    with pytest.raises(AssertionError) as exc_info:
        mock.assert_called(count=5)

    assert str(exc_info.value) == snapshot


async def test_assert_called_many_unmatched_requests_truncation(client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion) -> None:
    mock = client_mocker.get("http://api.example.com/specific").with_body_text("response")

    client_mocker.get(re.compile(r"http://api\.example\.com/different/.*")).with_body_text("different response")

    for i in range(8):
        await client.get(f"http://api.example.com/different/{i}") \
            .header("X-Request-ID", f"req-{i}") \
            .build_consumed().send()

    with pytest.raises(AssertionError) as exc_info:
        mock.assert_called()

    assert str(exc_info.value) == snapshot


async def test_assert_called_regex_matchers_display(client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion) -> None:
    path_pattern = re.compile(r"http://api\.example\.com/users/\d+")
    query_pattern = re.compile(r".*token=\w+.*")
    header_pattern = re.compile(r"Bearer [a-zA-Z0-9]{10,}")
    body_pattern = re.compile(r'.*"action":\s*"(create|update)".*')

    mock = client_mocker.put(path_pattern) \
        .match_query(query_pattern) \
        .match_header("Authorization", header_pattern) \
        .match_body(body_pattern) \
        .with_status(200)

    client_mocker.put("http://api.example.com/users/abc").with_status(400).with_body_text("Bad request")

    await client.put("http://api.example.com/users/abc") \
        .header("Authorization", "Bearer short") \
        .body_text('{"action": "delete"}') \
        .build_consumed().send()

    with pytest.raises(AssertionError) as exc_info:
        mock.assert_called()

    assert str(exc_info.value) == snapshot


async def test_assert_called_zero_count_success(client_mocker: ClientMocker, client: Client) -> None:
    mock = client_mocker.get("http://api.example.com/test").with_body_text("response")
    client_mocker.get("http://api.example.com/different").with_body_text("different response")

    await client.get("http://api.example.com/different").build_consumed().send()

    mock.assert_called(count=0)


async def test_assert_called_zero_count_failure(client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion) -> None:
    mock = client_mocker.get("http://api.example.com/test").with_body_text("response")

    await client.get("http://api.example.com/test").build_consumed().send()

    with pytest.raises(AssertionError) as exc_info:
        mock.assert_called(count=0)

    assert str(exc_info.value) == snapshot
