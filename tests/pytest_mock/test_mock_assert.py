import json
import re
from collections.abc import Generator

import pytest
from dirty_equals import Contains, IsInstance, IsPartialDict
from pyreqwest.client import Client, ClientBuilder
from pyreqwest.pytest_plugin import ClientMocker
from pyreqwest.request import Request
from pyreqwest.response import Response, ResponseBuilder
from syrupy import SnapshotAssertion  # type: ignore[attr-defined]

ANSI_REGEX = re.compile(r"\x1B\[[0-9;]*[mK]")


@pytest.fixture(autouse=True)
def set_verbosity(request: pytest.FixtureRequest) -> Generator[None]:
    # Set verbosity for mock assertion error snapshots
    prev_verbosity = request.config.option.verbose
    request.config.option.verbose = 2
    try:
        yield
    finally:
        request.config.option.verbose = prev_verbosity


@pytest.fixture
def client() -> Client:
    return ClientBuilder().build()


async def test_assert_called_default_exactly_once_success(client_mocker: ClientMocker, client: Client) -> None:
    mock = client_mocker.get(path="/test").with_body_text("response")

    await client.get("http://api.example.invalid/test").build().send()

    mock.assert_called()


async def test_assert_called_default_exactly_once_failure(
    client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion
) -> None:
    mock = client_mocker.get(path="/test").with_body_text("response")
    client_mocker.get(path="/different").with_body_text("different response")

    await client.get("http://api.example.invalid/different").build().send()

    with pytest.raises(AssertionError, match=re.escape("request(s) but received")) as exc_info:
        mock.assert_called()

    assert _clean_snapshot(str(exc_info.value)) == snapshot


async def test_assert_called_exact_count_success(client_mocker: ClientMocker, client: Client) -> None:
    mock = client_mocker.get(path="/test").with_body_text("response")

    for _ in range(3):
        await client.get("http://api.example.invalid/test").build().send()

    mock.assert_called(count=3)


async def test_assert_called_exact_count_failure(
    client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion
) -> None:
    mock = (
        client_mocker.post(path="/users")
        .match_header("Authorization", "Bearer token123")
        .match_body_json({"name": "John", "age": 30})
        .with_status(201)
        .with_body_json({"id": 1})
    )

    client_mocker.post(path="/users").with_status(403).with_body_text("Forbidden")
    client_mocker.get(path="/users").with_body_json({"users": []})

    res = (
        await client.post("http://api.example.invalid/users")
        .header("Authorization", "Bearer token123")
        .body_json({"name": "John", "age": 30})
        .build()
        .send()
    )
    assert await res.json() == {"id": 1}

    res = (
        await client.post("http://api.example.invalid/users")
        .header("Authorization", "Bearer wrong-token")
        .body_json({"name": "Jane", "age": 25})
        .build()
        .send()
    )
    assert res.status == 403

    res = await client.get("http://api.example.invalid/users").build().send()
    assert await res.json() == {"users": []}

    with pytest.raises(AssertionError, match=re.escape("request(s) but received")) as exc_info:
        mock.assert_called(count=3)

    assert _clean_snapshot(str(exc_info.value)) == snapshot


async def test_assert_called_min_count_success(client_mocker: ClientMocker, client: Client) -> None:
    mock = client_mocker.get(path="/test").with_body_text("response")

    for _ in range(5):
        await client.get("http://api.example.invalid/test").build().send()

    mock.assert_called(min_count=3)


async def test_assert_called_min_count_failure(
    client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion
) -> None:
    mock = client_mocker.get(path="/endpoint").match_query({"filter": "active"}).with_body_json({"data": []})

    client_mocker.get(path="/endpoint").with_body_json({"data": ["inactive"]})

    await client.get("http://api.example.invalid/endpoint?filter=active").build().send()
    await client.get("http://api.example.invalid/endpoint?filter=inactive").build().send()

    with pytest.raises(AssertionError, match=re.escape("request(s) but received")) as exc_info:
        mock.assert_called(min_count=3)

    assert _clean_snapshot(str(exc_info.value)) == snapshot


async def test_assert_called_max_count_success(client_mocker: ClientMocker, client: Client) -> None:
    mock = client_mocker.get(path="/test").with_body_text("response")

    for _ in range(2):
        await client.get("http://api.example.invalid/test").build().send()

    mock.assert_called(max_count=3)


async def test_assert_called_max_count_failure(
    client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion
) -> None:
    mock = client_mocker.get(path="/test").with_body_text("response")

    for _ in range(5):
        await client.get("http://api.example.invalid/test").build().send()

    with pytest.raises(AssertionError, match=re.escape("request(s) but received")) as exc_info:
        mock.assert_called(max_count=3)

    assert _clean_snapshot(str(exc_info.value)) == snapshot


async def test_assert_called_min_max_range_success(client_mocker: ClientMocker, client: Client) -> None:
    mock = client_mocker.get(path="/test").with_body_text("response")

    for _ in range(3):
        await client.get("http://api.example.invalid/test").build().send()

    mock.assert_called(min_count=2, max_count=5)


async def test_assert_called_min_max_range_failure(
    client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion
) -> None:
    mock = client_mocker.get(path="/test").with_body_text("response")

    await client.get("http://api.example.invalid/test").build().send()

    with pytest.raises(AssertionError, match=re.escape("request(s) but received")) as exc_info:
        mock.assert_called(min_count=3, max_count=5)

    assert _clean_snapshot(str(exc_info.value)) == snapshot


async def test_assert_called_complex_mock_with_all_matchers(
    client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion
) -> None:
    mock = (
        client_mocker.post(path="/complex")
        .match_header("Authorization", re.compile(r"Bearer \w+"))
        .match_header("Content-Type", "application/json")
        .match_query({"action": "create", "version": re.compile(r"v\d+")})
        .match_body_json({"user": {"name": "John", "role": "admin"}})
        .with_status(201)
    )

    client_mocker.get(path="/complex").with_status(405).with_body_text("Method not allowed")
    client_mocker.post(path="/complex").with_status(400).with_body_text("Bad request")

    requests_to_make = [
        # Wrong method
        ("GET", "http://api.example.invalid/complex?action=create&version=v1", {}, None),
        # Missing auth header
        (
            "POST",
            "http://api.example.invalid/complex?action=create&version=v1",
            {"Content-Type": "application/json"},
            {"user": {"name": "John", "role": "admin"}},
        ),
        # Wrong query param
        (
            "POST",
            "http://api.example.invalid/complex?action=update&version=v1",
            {"Authorization": "Bearer abc123", "Content-Type": "application/json"},
            {"user": {"name": "John", "role": "admin"}},
        ),
        # Wrong body
        (
            "POST",
            "http://api.example.invalid/complex?action=create&version=v1",
            {"Authorization": "Bearer abc123", "Content-Type": "application/json"},
            {"user": {"name": "Jane", "role": "user"}},
        ),
    ]

    for method, url, headers, body in requests_to_make:
        req_builder = client.request(method, url)
        for header_name, header_value in headers.items():
            req_builder = req_builder.header(header_name, header_value)
        if body is not None:
            req_builder = req_builder.body_json(body)
        await req_builder.build().send()

    with pytest.raises(AssertionError, match=re.escape("request(s) but received")) as exc_info:
        mock.assert_called()

    assert _clean_snapshot(str(exc_info.value)) == snapshot


async def test_assert_called_custom_matcher_and_handler(
    client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion
) -> None:
    async def is_admin_request(request: Request) -> bool:
        if request.body is None or (body_bytes := request.body.copy_bytes()) is None:
            return False
        body = json.loads(body_bytes.to_bytes())
        return isinstance(body, dict) and body.get("role") == "admin"

    async def admin_handler(_request: Request) -> Response:
        return await ResponseBuilder().status(200).body_json({"message": "Admin access granted"}).build()

    mock = client_mocker.post(path="/admin").match_request(is_admin_request).match_request_with_response(admin_handler)

    client_mocker.post(path="/admin").with_status(403).with_body_text("Forbidden")

    res = (
        await client.post("http://api.example.invalid/admin")
        .body_json({"role": "user", "action": "view"})
        .build()
        .send()
    )
    assert await res.text() == "Forbidden"

    res = (
        await client.post("http://api.example.invalid/admin")
        .body_json({"role": "admin", "action": "view"})
        .build()
        .send()
    )
    assert await res.json() == {"message": "Admin access granted"}

    with pytest.raises(AssertionError, match=re.escape("request(s) but received")) as exc_info:
        mock.assert_called(count=3)

    assert _clean_snapshot(str(exc_info.value)) == snapshot


async def test_assert_called_with_matched_and_unmatched_requests(
    client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion
) -> None:
    mock = client_mocker.get(path="/users").match_query({"active": "true"}).with_body_json({"users": []})

    client_mocker.get(path="/users").with_body_json({"users": ["inactive"]})
    client_mocker.get(path="/posts").with_body_json({"posts": []})

    for i in range(2):
        await client.get(f"http://api.example.invalid/users?active=true&page={i}").build().send()

    unmatched_requests = [
        "http://api.example.invalid/users?active=false",
        "http://api.example.invalid/users",
        "http://api.example.invalid/posts?active=true",
    ]

    for url in unmatched_requests:
        await client.get(url).build().send()

    with pytest.raises(AssertionError, match=re.escape("request(s) but received")) as exc_info:
        mock.assert_called(count=5)

    assert _clean_snapshot(str(exc_info.value)) == snapshot


async def test_assert_called_many_unmatched_requests_truncation(
    client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion
) -> None:
    mock = client_mocker.get(path="/specific").with_body_text("response")

    client_mocker.get(path=re.compile(r"/different/.*")).with_body_text("different response")

    for i in range(8):
        await client.get(f"http://api.example.invalid/different/{i}").header("X-Request-ID", f"req-{i}").build().send()

    with pytest.raises(AssertionError, match=re.escape("request(s) but received")) as exc_info:
        mock.assert_called()

    assert _clean_snapshot(str(exc_info.value)) == snapshot


async def test_assert_called_regex_matchers_display(
    client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion
) -> None:
    path_pattern = re.compile(r"/users/\d+")
    query_pattern = re.compile(r".*token=\w+.*")
    header_pattern = re.compile(r"Bearer [a-zA-Z0-9]{10,}")
    body_pattern = re.compile(r'.*"action":\s*"(create|update)".*')

    mock = (
        client_mocker.put(path=path_pattern)
        .match_query(query_pattern)
        .match_header("Authorization", header_pattern)
        .match_body(body_pattern)
        .with_status(200)
    )

    client_mocker.put(path="/users/abc").with_status(400).with_body_text("Bad request")

    await (
        client.put("http://api.example.invalid/users/abc")
        .header("Authorization", "Bearer short")
        .body_text('{"action": "delete"}')
        .build()
        .send()
    )

    with pytest.raises(AssertionError, match=re.escape("request(s) but received")) as exc_info:
        mock.assert_called()

    assert _clean_snapshot(str(exc_info.value)) == snapshot


async def test_assert_called_zero_count_success(client_mocker: ClientMocker, client: Client) -> None:
    mock = client_mocker.get(path="/test").with_body_text("response")
    client_mocker.get(path="/different").with_body_text("different response")

    await client.get("http://api.example.invalid/different").build().send()

    mock.assert_called(count=0)


async def test_assert_called_zero_count_failure(
    client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion
) -> None:
    mock = client_mocker.get(path="/test").with_body_text("response")

    await client.get("http://api.example.invalid/test").build().send()

    with pytest.raises(AssertionError, match=re.escape("request(s) but received")) as exc_info:
        mock.assert_called(count=0)

    assert _clean_snapshot(str(exc_info.value)) == snapshot


async def test_dirty_equals_matcher_repr(
    client_mocker: ClientMocker, client: Client, snapshot: SnapshotAssertion
) -> None:
    mock = client_mocker.get(path="/test").match_query(
        IsPartialDict({"values": IsInstance(list) & Contains("admin", "user")}),
    )
    others = client_mocker.get(path="/test")

    await client.get("http://api.example.invalid/test").build().send()
    await client.get("http://api.example.invalid/test?values=foo&values=admin&values=user").build().send()
    await client.get("http://api.example.invalid/test?values=admin").build().send()

    mock.assert_called(count=1)
    others.assert_called(count=2)

    with pytest.raises(AssertionError, match=re.escape("request(s) but received")) as exc_info:
        mock.assert_called(count=2)

    assert _clean_snapshot(str(exc_info.value)) == snapshot


def _clean_snapshot(val: str) -> str:
    res = ANSI_REGEX.sub("", val)  # Remove ANSI codes
    if "Differing items:" in res:
        # Sort the "Differing items" section for consistent snapshots
        lines = res.splitlines()
        s = next(i for i, line in enumerate(lines) if "Differing items:" in line)
        e = next(i for i, line in enumerate(lines) if "Full diff:" in line) - 1
        res = "\n".join(lines[:s] + sorted(lines[s:e]) + lines[e:])
    return res
