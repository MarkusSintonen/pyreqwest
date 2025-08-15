from typing import Pattern, Self, TypedDict, Unpack, assert_never

import pytest

from pyreqwest.client import Client
from pyreqwest.http import Body, Url
from pyreqwest.middleware import Next
from pyreqwest.request import Request, RequestBuilder
from pyreqwest.response import Response, ResponseBuilder
from pyreqwest.types import Middleware

UrlMatcher = str | Url | Pattern[str]


class MockOpts(TypedDict, total=False):
    url: str | Pattern[str]
    headers: dict[str, str | Pattern[str]]
    body: str | bytes | Pattern[str]


class RequestMatcher:
    """Matcher for HTTP requests."""

    def __init__(self, method: str | None = None, url: UrlMatcher | None = None, **kwargs: Unpack[MockOpts]) -> None:
        self.method = method
        self.url = url
        self.headers = kwargs.get("headers") or {}
        self.body = kwargs.get("body")

    def matches(self, request: Request) -> bool:
        """Check if the request matches this matcher."""
        return (
            self._matches_method(request)
            and self._matches_url(request)
            and self._matches_headers(request)
            and self._matches_body(request)
        )

    def _matches_method(self, request: Request) -> bool:
        return self.method is None or request.method == self.method

    def _matches_url(self, request: Request) -> bool:
        if self.url is None:
            return True
        elif isinstance(self.url, str | Url):
            return request.url == self.url
        elif isinstance(self.url, Pattern):
            return self.url.search(str(request.url)) is not None
        else:
            assert_never(self.url)

    def _matches_headers(self, request: Request) -> bool:
        for header_name, expected_value in self.headers.items():
            actual_value = request.headers.get(header_name)
            if actual_value is None:
                return False
            elif isinstance(expected_value, Pattern):
                if not expected_value.search(actual_value):
                    return False
            elif actual_value != expected_value:
                return False

        return True

    def _matches_body(self, request: Request) -> bool:
        if self.body is None:
            return True

        body_bytes = request.body.copy_bytes() if request.body is not None else None
        if body_bytes is None:
            return False

        body_data = bytes(body_bytes)

        if isinstance(self.body, bytes):
            return body_data == self.body
        elif isinstance(self.body, str):
            return body_data.decode() == self.body
        elif isinstance(self.body, Pattern):
            return self.body.search(body_data.decode()) is not None
        else:
            assert_never(self.body)


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
        self, method: str | None = None, url: UrlMatcher | None = None, **kwargs: Unpack[MockOpts]
    ) -> ResponseBuilder:
        """Add a mock rule for requests matching the given criteria."""
        matcher = RequestMatcher(method, url, **kwargs)
        response_builder = ResponseBuilder.create_for_mocking()
        rule = MockRule(matcher, response_builder)
        self._rules.append(rule)
        return response_builder

    def get(self, url: UrlMatcher | None = None, **kwargs: Unpack[MockOpts]) -> ResponseBuilder:
        """Mock GET requests to the given URL."""
        return self.mock("GET", url, **kwargs)

    def post(self, url: UrlMatcher | None = None, **kwargs: Unpack[MockOpts]) -> ResponseBuilder:
        """Mock POST requests to the given URL."""
        return self.mock("POST", url, **kwargs)

    def put(self, url: UrlMatcher | None = None, **kwargs: Unpack[MockOpts]) -> ResponseBuilder:
        """Mock PUT requests to the given URL."""
        return self.mock("PUT", url, **kwargs)

    def patch(self, url: UrlMatcher | None = None, **kwargs: Unpack[MockOpts]) -> ResponseBuilder:
        """Mock PATCH requests to the given URL."""
        return self.mock("PATCH", url, **kwargs)

    def delete(self, url: UrlMatcher | None = None, **kwargs: Unpack[MockOpts]) -> ResponseBuilder:
        """Mock DELETE requests to the given URL."""
        return self.mock("DELETE", url, **kwargs)

    def head(self, url: UrlMatcher | None = None, **kwargs: Unpack[MockOpts]) -> ResponseBuilder:
        """Mock HEAD requests to the given URL."""
        return self.mock("HEAD", url, **kwargs)

    def options(self, url: UrlMatcher | None = None, **kwargs: Unpack[MockOpts]) -> ResponseBuilder:
        """Mock OPTIONS requests to the given URL."""
        return self.mock("OPTIONS", url, **kwargs)

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
