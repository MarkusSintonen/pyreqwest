"""Pytest mocking utilities for pyreqwest client."""

from __future__ import annotations

import re
from typing import Any, Callable, Dict, List, Optional, Pattern, Union
from unittest.mock import MagicMock

import pytest

from pyreqwest.client import Client, ClientBuilder
from pyreqwest.http import Body
from pyreqwest.middleware import Next
from pyreqwest.request import Request
from pyreqwest.response import Response
from pyreqwest.types import Middleware


class MockResponse:
    """Builder for mock HTTP responses."""

    def __init__(self) -> None:
        self._status: int = 200
        self._body: Optional[Body] = None
        self._headers: Dict[str, str] = {}
        self._version: str = "HTTP/1.1"

    def status(self, status: int) -> MockResponse:
        """Set the response status code."""
        self._status = status
        return self

    def body(self, body: Body) -> MockResponse:
        """Set the response body."""
        self._body = body
        return self

    def text(self, text: str) -> MockResponse:
        """Set the response body as text."""
        self._body = Body.from_text(text)
        return self

    def json(self, data: Any) -> MockResponse:
        """Set the response body as JSON."""
        import json
        self._body = Body.from_text(json.dumps(data))
        self.header("content-type", "application/json")
        return self

    def bytes(self, data: bytes) -> MockResponse:
        """Set the response body as bytes."""
        self._body = Body.from_bytes(data)
        return self

    def header(self, name: str, value: str) -> MockResponse:
        """Add a response header."""
        self._headers[name] = value
        return self

    def headers(self, headers: Dict[str, str]) -> MockResponse:
        """Set multiple response headers."""
        self._headers.update(headers)
        return self

    def version(self, version: str) -> MockResponse:
        """Set the HTTP version."""
        self._version = version
        return self


class RequestMatcher:
    """Matcher for HTTP requests."""

    def __init__(
        self,
        method: Optional[str] = None,
        url: Optional[Union[str, Pattern[str]]] = None,
        headers: Optional[Dict[str, Union[str, Pattern[str]]]] = None,
        body: Optional[Union[str, bytes, Pattern[str]]] = None,
    ) -> None:
        self.method = method
        self.url = url if isinstance(url, (type(None), Pattern)) else re.compile(re.escape(url))
        self.headers = headers or {}
        self.body = body

    def matches(self, request: Request) -> bool:
        """Check if the request matches this matcher."""
        if self.method and request.method != self.method:
            return False

        if self.url and not self.url.search(str(request.url)):
            return False

        for header_name, expected_value in self.headers.items():
            actual_value = request.headers.get(header_name)
            if actual_value is None:
                return False

            if isinstance(expected_value, Pattern):
                if not expected_value.search(actual_value):
                    return False
            elif actual_value != expected_value:
                return False

        if self.body is not None and request.body is not None:
            body_bytes = request.body.copy_bytes()
            if body_bytes is None:
                return False

            body_data = bytes(body_bytes)

            if isinstance(self.body, bytes):
                return body_data == self.body
            elif isinstance(self.body, str):
                return body_data.decode() == self.body
            elif isinstance(self.body, Pattern):
                return self.body.search(body_data.decode()) is not None

        return True


class MockRule:
    """A single mock rule that matches requests and returns responses."""

    def __init__(self, matcher: RequestMatcher, response: MockResponse) -> None:
        self.matcher = matcher
        self.response = response
        self.call_count = 0
        self.requests: List[Request] = []

    async def handle(self, request: Request, next_handler: Next) -> Response:
        """Handle a matching request."""
        self.call_count += 1
        self.requests.append(request.copy())

        # Create a fresh body for each response to avoid "already consumed" errors
        if self.response._body is not None:
            # Get the original body data and create a new Body instance
            if hasattr(self.response._body, 'copy_bytes'):
                body_data = self.response._body.copy_bytes()
                if body_data is not None:
                    body = Body.from_bytes(body_data)
                else:
                    body = Body.from_text("")
            else:
                body = Body.from_text("")
        else:
            body = Body.from_text("")

        response = await next_handler.override_response_builder() \
            .status(self.response._status) \
            .body(body) \
            .build()

        for name, value in self.response._headers.items():
            response.headers[name] = value

        return response


class ClientMocker:
    """Main class for mocking HTTP requests."""

    def __init__(self) -> None:
        self._rules: List[MockRule] = []
        self._default_response: Optional[MockResponse] = None
        self._strict = False

    def mock(
        self,
        method: Optional[str] = None,
        url: Optional[Union[str, Pattern[str]]] = None,
        headers: Optional[Dict[str, Union[str, Pattern[str]]]] = None,
        body: Optional[Union[str, bytes, Pattern[str]]] = None,
    ) -> MockResponse:
        """Add a mock rule for requests matching the given criteria."""
        matcher = RequestMatcher(method=method, url=url, headers=headers, body=body)
        response = MockResponse()
        rule = MockRule(matcher, response)
        self._rules.append(rule)
        return response

    def get(self, url: Union[str, Pattern[str]]) -> MockResponse:
        """Mock GET requests to the given URL."""
        return self.mock(method="GET", url=url)

    def post(self, url: Union[str, Pattern[str]]) -> MockResponse:
        """Mock POST requests to the given URL."""
        return self.mock(method="POST", url=url)

    def put(self, url: Union[str, Pattern[str]]) -> MockResponse:
        """Mock PUT requests to the given URL."""
        return self.mock(method="PUT", url=url)

    def patch(self, url: Union[str, Pattern[str]]) -> MockResponse:
        """Mock PATCH requests to the given URL."""
        return self.mock(method="PATCH", url=url)

    def delete(self, url: Union[str, Pattern[str]]) -> MockResponse:
        """Mock DELETE requests to the given URL."""
        return self.mock(method="DELETE", url=url)

    def head(self, url: Union[str, Pattern[str]]) -> MockResponse:
        """Mock HEAD requests to the given URL."""
        return self.mock(method="HEAD", url=url)

    def options(self, url: Union[str, Pattern[str]]) -> MockResponse:
        """Mock OPTIONS requests to the given URL."""
        return self.mock(method="OPTIONS", url=url)

    def default_response(self) -> MockResponse:
        """Set a default response for unmatched requests."""
        self._default_response = MockResponse()
        return self._default_response

    def strict(self, enabled: bool = True) -> ClientMocker:
        """Enable strict mode - unmatched requests will raise an error."""
        self._strict = enabled
        return self

    def get_requests(self, method: Optional[str] = None, url: Optional[Union[str, Pattern[str]]] = None) -> List[Request]:
        """Get all captured requests, optionally filtered by method and URL."""
        requests = []
        for rule in self._rules:
            for request in rule.requests:
                if method and request.method != method:
                    continue
                if url:
                    url_pattern = url if isinstance(url, Pattern) else re.compile(re.escape(url))
                    if not url_pattern.search(str(request.url)):
                        continue
                requests.append(request)
        return requests

    def get_call_count(self, method: Optional[str] = None, url: Optional[Union[str, Pattern[str]]] = None) -> int:
        """Get the total number of calls, optionally filtered by method and URL."""
        count = 0
        for rule in self._rules:
            if method or url:
                # Filter by method/URL in the rule's matcher
                matcher = RequestMatcher(method=method, url=url)
                for request in rule.requests:
                    if matcher.matches(request):
                        count += 1
            else:
                count += rule.call_count
        return count

    def reset(self) -> None:
        """Reset all mock rules and captured requests."""
        for rule in self._rules:
            rule.call_count = 0
            rule.requests.clear()

    def _create_middleware(self) -> Middleware:
        """Create the middleware function for request interception."""
        async def mock_middleware(_client: Client, request: Request, next_handler: Next) -> Response:
            # Try to find a matching rule
            for rule in self._rules:
                if rule.matcher.matches(request):
                    return await rule.handle(request, next_handler)

            # No rule matched
            if self._default_response:
                rule = MockRule(RequestMatcher(), self._default_response)
                return await rule.handle(request, next_handler)
            elif self._strict:
                raise AssertionError(f"No mock rule matched request: {request.method} {request.url}")
            else:
                # Let the request proceed normally
                return await next_handler.run(request)

        return mock_middleware


@pytest.fixture
def client_mocker(monkeypatch: pytest.MonkeyPatch) -> ClientMocker:
    """Pytest fixture that provides a ClientMocker instance."""
    mocker = ClientMocker()
    mocked_ids: set[int] = set()
    orig_build = ClientBuilder.build

    def build_patch(self: ClientBuilder) -> Client:
        if id(self) in mocked_ids:  # Break recursion
            mocked_ids.remove(id(self))
            return orig_build(self)

        mocked_ids.add(id(self))
        return self.with_middleware(mocker._create_middleware()).build()

    monkeypatch.setattr(ClientBuilder, "build", build_patch)
    return mocker
