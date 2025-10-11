"""Mocks usage.

In pytest you can use the `client_mocker` fixture that provides ClientMocker.
You can also see more examples in the tests/pytest_mock folder.

Run directly:
    uv run python -m examples.testing
"""

import json
import re
from collections.abc import Generator
from contextlib import contextmanager
from typing import Any

import pytest
from dirty_equals import IsDict, IsPartialDict
from pyreqwest.client import ClientBuilder
from pyreqwest.pytest_plugin import ClientMocker
from pyreqwest.request import Request
from pyreqwest.response import Response, ResponseBuilder

# In pytest you can use the `client_mocker` fixture that provides ClientMocker.


async def example_basic_mocking() -> None:
    """Basic example of mocking"""
    with client_mocker_ctx() as client_mocker:
        # mock exact URL and returning a response body and default 200 status
        mock1 = client_mocker.get(url="https://example.invalid/api").with_body_json({"res": 42})
        # mock path and returning a response body with a defined status
        mock2 = client_mocker.get(path="/api").with_body_json({"res": 43}).with_status(202)

        await request("GET", "https://example.invalid/api")
        await request("GET", "https://example2.invalid/api")

        assert mock1.get_call_count() == 1
        assert mock2.get_call_count() == 1


async def example_path_regex_mocking() -> None:
    """Example showing regex matching on path"""
    with client_mocker_ctx() as client_mocker:
        mock1 = client_mocker.get(path=re.compile(r"^/api/item/\d+$")).with_body_json({"res": 42})
        mock2 = client_mocker.get(path=re.compile(r"^/api/item/\d+/detail$")).with_body_json({"res": 43})

        await request("GET", "https://example.invalid/api/item/123")
        await request("GET", "https://example.invalid/api/item/234/detail")

        assert mock1.get_call_count() == 1
        assert mock2.get_call_count() == 1


async def example_query_mocking() -> None:
    """Example showing query parameter matching"""
    with client_mocker_ctx() as client_mocker:
        mock1 = client_mocker.get().match_query({"q": "A"}).with_body_json({"res": 42})
        mock2 = client_mocker.get().match_query({"q": re.compile(r"^\d+$")}).with_body_json({"res": 43})

        await request("GET", "https://example.invalid/api?q=A")
        await request("GET", "https://example.invalid/api?q=10")
        await request("GET", "https://example.invalid/api?q=A&extra=B")  # with extra param

        assert mock1.get_call_count() == 2
        assert mock2.get_call_count() == 1


async def example_matching_compatibility() -> None:
    """Example showing how matchers are compatible with other matcher libraries. Like dirty-equals."""
    with client_mocker_ctx(strict=True) as client_mocker:
        mock1 = client_mocker.get().match_query(IsPartialDict({"q": "A"})).with_body_json({"res": 42})
        mock2 = client_mocker.get().match_query(IsDict({"q": "B"})).with_body_json({"res": 43})

        await request("GET", "https://example.invalid/api?q=A&extra=1")
        await request("GET", "https://example.invalid/api?q=B")

        try:
            # should not match the exact IsDict
            await request("GET", "https://example.invalid/api?q=B&extra=1")
        except AssertionError as e:
            assert str(e) == "No mock rule matched request: GET https://example.invalid/api?q=B&extra=1"
            print(e)

        assert mock1.get_call_count() == 1
        assert mock2.get_call_count() == 1


async def example_body_matching() -> None:
    """Example showing body matching"""
    with client_mocker_ctx() as client_mocker:
        mock1 = client_mocker.post().match_body_json(IsPartialDict({"param": "A"})).with_body_json({"res": 42})
        mock2 = client_mocker.post().match_body_json({"param": "B"}).with_body_json({"res": 43})
        unmatched = client_mocker.mock()

        await request("POST", "https://example.invalid/api", json={"param": "A", "extra": 1})
        await request("POST", "https://example.invalid/api", json={"param": "B"})

        # should not match the exact dict
        await request("POST", "https://example.invalid/api", json={"param": "B", "extra": 1})

        assert mock1.get_call_count() == 1
        assert mock2.get_call_count() == 1
        assert unmatched.get_call_count() == 1
        req = unmatched.get_requests()[0]
        assert req.body and json.loads(bytes(req.body.copy_bytes() or b"")) == {"param": "B", "extra": 1}


async def example_custom_predicate_matcher() -> None:
    """Example showing custom matcher usage"""

    async def matcher(request: Request) -> bool:  # Is also works with sync functions
        return request.method in ("GET", "POST")

    with client_mocker_ctx() as client_mocker:
        mock = client_mocker.mock().match_request(matcher).with_body_json({"res": 42})
        unmatched = client_mocker.mock()

        await request("GET", "https://example.invalid/api")
        await request("POST", "https://example.invalid/api")
        await request("DELETE", "https://example.invalid/api")

        assert mock.get_call_count() == 2
        assert unmatched.get_call_count() == 1
        assert unmatched.get_requests()[0].method == "DELETE"


async def example_custom_matcher_with_response() -> None:
    """Example showing custom matcher usage"""

    async def matcher(request: Request) -> Response | None:  # Is also works with sync functions
        if request.method not in ("GET", "POST"):
            return None  # No match
        return await ResponseBuilder().status(201).body_json({"res": 42, "method_was": request.method}).build()

    with client_mocker_ctx() as client_mocker:
        mock = client_mocker.mock().match_request_with_response(matcher)
        unmatched = client_mocker.mock()

        await request("GET", "https://example.invalid/api")
        await request("POST", "https://example.invalid/api")
        await request("DELETE", "https://example.invalid/api")

        assert mock.get_call_count() == 2
        assert unmatched.get_call_count() == 1
        assert unmatched.get_requests()[0].method == "DELETE"


async def example_mock_ordering() -> None:
    """Example showing that the order of initialization with mocking does not matter."""
    async with ClientBuilder().build() as client:
        with client_mocker_ctx() as client_mocker:
            client_mocker.get(url="https://example.invalid/api")

            response = await client.request("GET", "https://example.invalid/api").build().send()
            print({"status": response.status, "body": await response.text()})


@contextmanager
def client_mocker_ctx(strict: bool = True) -> Generator[ClientMocker, None, None]:
    """Context manager version of the client_mocker pytest fixture which is available."""
    with pytest.MonkeyPatch.context() as patcher:
        yield ClientMocker.create_mocker(patcher).strict(strict)


async def request(method: str, url: str, json: Any | None = None) -> None:
    """Helper function to make a request and print the response for snapshot testing"""
    async with ClientBuilder().build() as client:
        req = client.request(method, url)

        if json is not None:
            req = req.body_json(json)

        response = await req.build().send()
        print({"status": response.status, "body": await response.text()})
