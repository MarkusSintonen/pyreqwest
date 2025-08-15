"""Pytest mocking utilities for pyreqwest client."""

import re
from typing import Pattern, Self

import pytest

from pyreqwest.client import Client, ClientBuilder
from pyreqwest.http import Body
from pyreqwest.middleware import Next
from pyreqwest.request import Request, RequestBuilder
from pyreqwest.response import Response, ResponseBuilder
from pyreqwest.types import Middleware


class RequestMatcher:
    """Matcher for HTTP requests."""

    def __init__(
        self,
        method: str | None = None,
        url: str | Pattern[str] | None = None,
        headers: dict[str, str | Pattern[str]] | None = None,
        body: str | bytes | Pattern[str] | None = None,
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

    def __init__(self, matcher: RequestMatcher, response_builder: ResponseBuilder) -> None:
        self.matcher = matcher
        self.requests: list[Request] = []
        self._response_builder = response_builder
        self._built_response: Response | None = None

    async def response(self) -> Response:
        if self._built_response is None:
            self._built_response = await self._response_builder.build()
        return self._built_response

    async def handle(self, request: Request) -> Response:
        self.requests.append(request)
        return await self.response()


class ClientMocker:
    """Main class for mocking HTTP requests."""

    def __init__(self) -> None:
        self._rules: list[MockRule] = []
        self._strict = False

    def mock(
        self,
        method: str | None = None,
        url: str | Pattern[str] | None = None,
        headers: dict[str, str | Pattern[str]] | None = None,
        body: str | bytes | Pattern[str] | None = None,
    ) -> ResponseBuilder:
        """Add a mock rule for requests matching the given criteria."""
        matcher = RequestMatcher(method=method, url=url, headers=headers, body=body)
        response_builder = ResponseBuilder.create_for_mocking()
        rule = MockRule(matcher, response_builder)
        self._rules.append(rule)
        return response_builder

    def get(self, url: str | Pattern[str]) -> ResponseBuilder:
        """Mock GET requests to the given URL."""
        return self.mock(method="GET", url=url)

    def post(self, url: str | Pattern[str]) -> ResponseBuilder:
        """Mock POST requests to the given URL."""
        return self.mock(method="POST", url=url)

    def put(self, url: str | Pattern[str]) -> ResponseBuilder:
        """Mock PUT requests to the given URL."""
        return self.mock(method="PUT", url=url)

    def patch(self, url: str | Pattern[str]) -> ResponseBuilder:
        """Mock PATCH requests to the given URL."""
        return self.mock(method="PATCH", url=url)

    def delete(self, url: str | Pattern[str]) -> ResponseBuilder:
        """Mock DELETE requests to the given URL."""
        return self.mock(method="DELETE", url=url)

    def head(self, url: str | Pattern[str]) -> ResponseBuilder:
        """Mock HEAD requests to the given URL."""
        return self.mock(method="HEAD", url=url)

    def options(self, url: str | Pattern[str]) -> ResponseBuilder:
        """Mock OPTIONS requests to the given URL."""
        return self.mock(method="OPTIONS", url=url)

    def strict(self, enabled: bool = True) -> Self:
        """Enable strict mode - unmatched requests will raise an error."""
        self._strict = enabled
        return self

    def _filter_requests(self, method: str | None = None, url: str | Pattern[str] | None = None) -> list[Request]:
        """Filter all captured requests by method and URL."""
        if not method and not url:
            # Return all requests if no filters
            return [request for rule in self._rules for request in rule.requests]

        # Create a matcher for filtering
        filter_matcher = RequestMatcher(method=method, url=url)
        filtered_requests = []

        for rule in self._rules:
            for request in rule.requests:
                if filter_matcher.matches(request):
                    filtered_requests.append(request)

        return filtered_requests

    def get_requests(self, method: str | None = None, url: str | Pattern[str] | None = None) -> list[Request]:
        """Get all captured requests, optionally filtered by method and URL."""
        return self._filter_requests(method, url)

    def get_call_count(self, method: str | None = None, url: str | Pattern[str] | None = None) -> int:
        """Get the total number of calls, optionally filtered by method and URL."""
        return len(self._filter_requests(method, url))

    def reset(self) -> None:
        """Reset all mock rules and captured requests."""
        for rule in self._rules:
            rule.requests.clear()

    def _create_middleware(self) -> Middleware:
        async def mock_middleware(_client: Client, request: Request, next_handler: Next) -> Response:
            if request.body is not None and (stream := request.body.get_stream()) is not None:
                body = [bytes(chunk) async for chunk in stream]  # Read the body stream into bytes
                request = request.from_request_and_body(request, Body.from_bytes(b"".join(body)))

            for rule in self._rules:
                if rule.matcher.matches(request):
                    return await rule.handle(request)

            # No rule matched
            if self._strict:
                raise AssertionError(f"No mock rule matched request: {request.method} {request.url}")
            else:
                # Let the request proceed normally
                return await next_handler.run(request)

        return mock_middleware


@pytest.fixture
def client_mocker(monkeypatch: pytest.MonkeyPatch) -> ClientMocker:
    mocker = ClientMocker()

    orig_build_consumed = RequestBuilder.build_consumed
    orig_build_streamed = RequestBuilder.build_streamed

    def build_patch(self: RequestBuilder, orig) -> Request:
        request = orig(self)
        assert request._interceptor is None
        request._interceptor = mocker._create_middleware()
        return request

    monkeypatch.setattr(RequestBuilder, "build_consumed", lambda slf: build_patch(slf, orig_build_consumed))
    monkeypatch.setattr(RequestBuilder, "build_streamed", lambda slf: build_patch(slf, orig_build_streamed))
    return mocker
