from functools import cached_property
from typing import Pattern, Self, assert_never, Callable, Awaitable, Any

import pytest

from pyreqwest.client import Client
from pyreqwest.http import Body, Url
from pyreqwest.middleware import Next
from pyreqwest.request import Request, RequestBuilder
from pyreqwest.response import Response, ResponseBuilder
from pyreqwest.types import Middleware

MethodMatcher = str | set[str]
UrlMatcher = str | Url | Pattern[str]
QueryMatcher = dict[str, str | Pattern[str]] | Pattern[str]
BodyMatcher = str | bytes | Pattern[str]
CustomMatcher = Callable[[Request], Awaitable[bool]]
CustomHandler = Callable[[Request], Awaitable[Response | None]]


class Mock:
    def __init__(self, method: MethodMatcher | None = None, url: UrlMatcher | None = None) -> None:
        self._method_matcher = method
        self._url_matcher = url
        self._query_matcher: QueryMatcher | None = None
        self._header_matchers: dict[str, str | Pattern[str]] = {}
        self._body_matcher: BodyMatcher | None = None
        self._custom_matcher: CustomMatcher | None = None
        self._custom_handler: CustomHandler | None = None

        self._matched_requests: list[Request] = []

        self._using_response_builder = False
        self._built_response: Response | None = None

    def get_requests(self) -> list[Request]:
        """Get all captured requests by this mock"""
        return [*self._matched_requests]

    def get_call_count(self) -> int:
        """Get the total number of calls to this mock"""
        return len(self._matched_requests)

    def reset_requests(self) -> None:
        """Reset all captured requests for this mock"""
        self._matched_requests.clear()

    def match_url(self, matcher: Pattern[str]) -> Self:
        self._url_matcher = matcher
        return self

    def match_query(self, matcher: QueryMatcher) -> Self:
        self._query_matcher = matcher
        return self

    def match_header(self, name: str, value: str | Pattern[str]) -> Self:
        self._header_matchers[name] = value
        return self

    def match_body(self, matcher: BodyMatcher) -> Self:
        self._body_matcher = matcher
        return self

    def match_request(self, matcher: CustomMatcher) -> Self:
        self._custom_matcher = matcher
        return self

    def match_request_with_response(self, handler: CustomHandler) -> Self:
        self._custom_handler = handler
        return self

    def with_status(self, status: int) -> Self:
        self._response_builder.status(status)
        return self

    def with_header(self, name: str, value: str) -> Self:
        self._response_builder.header(name, value)
        return self

    def with_body(self, body: Body) -> Self:
        self._response_builder.body(body)
        return self

    def with_body_bytes(self, body: bytes | bytearray | memoryview) -> Self:
        self._response_builder.body_bytes(body)
        return self

    def with_body_text(self, body: str) -> Self:
        self._response_builder.body_text(body)
        return self

    def with_body_json(self, json: Any) -> Self:
        self._response_builder.body_json(json)
        return self

    def with_version(self, version: str) -> Self:
        self._response_builder.version(version)
        return self

    async def _handle(self, request: Request) -> Response | None:
        if self._using_response_builder and self._custom_handler is not None:
            raise AssertionError("Cannot use both response builder and custom handler in the same mock")

        matched = (
            self._matches_method(request)
            and self._matches_url(request)
            and self._match_headers(request)
            and self._match_body(request)
            and await self._matches_custom(request)
        )
        if not matched:
            return None

        response = (
            await self._custom_handler(request)
            if self._custom_handler is not None
            else await self._response()
        )
        if response is None:
            return None

        self._matched_requests.append(request)
        return response

    @cached_property
    def _response_builder(self) -> ResponseBuilder:
        self._using_response_builder = True
        return ResponseBuilder.create_for_mocking()

    async def _response(self) -> Response:
        if self._built_response is None:
            self._built_response = await self._response_builder.build()
        return self._built_response

    def _matches_method(self, request: Request) -> bool:
        if self._method_matcher is None:
            return True
        elif isinstance(self._method_matcher, str):
            return request.method == self._method_matcher
        elif isinstance(self._method_matcher, set):
            return request.method in self._method_matcher
        else:
            assert_never(self._method_matcher)

    def _matches_url(self, request: Request) -> bool:
        if self._url_matcher is None:
            return True
        if isinstance(self._url_matcher, str | Url):
            if request.url == self._url_matcher:
                return True
        elif isinstance(self._url_matcher, Pattern):
            if self._url_matcher.search(str(request.url)) is not None:
                return True
        else:
            assert_never(self._url_matcher)
        return False

    def _match_headers(self, request: Request) -> bool:
        for header_name, expected_value in self._header_matchers.items():
            actual_value = request.headers.get(header_name)
            if actual_value is None:
                return False
            if isinstance(expected_value, str):
                if actual_value != expected_value:
                    return False
            elif isinstance(expected_value, Pattern):
                if not expected_value.search(actual_value):
                    return False
            else:
                assert_never(expected_value)
        return True

    def _match_body(self, request: Request) -> bool:
        if self._body_matcher is None:
            return True

        if request.body is None:
            return False

        assert request.body.get_stream() is None, "Stream should have been consumed into body bytes by mock middleware"
        body_bytes = request.body.copy_bytes()
        assert body_bytes is not None, "Unknown body type"

        if isinstance(self._body_matcher, bytes):
            if body_bytes.to_bytes() != self._body_matcher:
                return False
        elif isinstance(self._body_matcher, str):
            if body_bytes.to_bytes().decode() != self._body_matcher:
                return False
        elif isinstance(self._body_matcher, Pattern):
            if self._body_matcher.search(body_bytes.to_bytes().decode()) is None:
                return False
        else:
            assert_never(self._body_matcher)
        return True

    async def _matches_custom(self, request: Request) -> bool:
        if self._custom_matcher is None:
            return True
        return await self._custom_matcher(request)


class ClientMocker:
    """Main class for mocking HTTP requests."""

    def __init__(self) -> None:
        self._mocks: list[Mock] = []
        self._strict = False

    def mock(self, method: MethodMatcher | None = None, url: UrlMatcher | None = None) -> Mock:
        """Add a mock rule for requests matching the given criteria."""
        mock = Mock(method, url)
        self._mocks.append(mock)
        return mock

    def get(self, url: UrlMatcher | None = None) -> Mock:
        """Mock GET requests to the given URL."""
        return self.mock("GET", url)

    def post(self, url: UrlMatcher | None = None) -> Mock:
        """Mock POST requests to the given URL."""
        return self.mock("POST", url)

    def put(self, url: UrlMatcher | None = None) -> Mock:
        """Mock PUT requests to the given URL."""
        return self.mock("PUT", url)

    def patch(self, url: UrlMatcher | None = None) -> Mock:
        """Mock PATCH requests to the given URL."""
        return self.mock("PATCH", url)

    def delete(self, url: UrlMatcher | None = None) -> Mock:
        """Mock DELETE requests to the given URL."""
        return self.mock("DELETE", url)

    def head(self, url: UrlMatcher | None = None) -> Mock:
        """Mock HEAD requests to the given URL."""
        return self.mock("HEAD", url)

    def options(self, url: UrlMatcher | None = None) -> Mock:
        """Mock OPTIONS requests to the given URL."""
        return self.mock("OPTIONS", url)

    def strict(self, enabled: bool = True) -> Self:
        """Enable strict mode - unmatched requests will raise an error."""
        self._strict = enabled
        return self

    def get_requests(self) -> list[Request]:
        """Get all captured requests in all mocks"""
        return [request for mock in self._mocks for request in mock.get_requests()]

    def get_call_count(self) -> int:
        """Get the total number of calls in all mocks"""
        return sum(mock.get_call_count() for mock in self._mocks)

    def clear(self) -> None:
        """Remove all mocks"""
        self._mocks.clear()

    def reset_requests(self) -> None:
        """Reset all captured requests in all mocks"""
        for mock in self._mocks:
            mock.reset_requests()

    def _create_middleware(self) -> Middleware:
        async def mock_middleware(_client: Client, request: Request, next_handler: Next) -> Response:
            if request.body is not None and (stream := request.body.get_stream()) is not None:
                body = [bytes(chunk) async for chunk in stream]  # Read the body stream into bytes
                request = request.from_request_and_body(request, Body.from_bytes(b"".join(body)))

            for mock in self._mocks:
                if (response := await mock._handle(request)) is not None:
                    return response

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
