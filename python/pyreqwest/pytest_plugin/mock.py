import json
from functools import cached_property
from typing import Pattern, Self, Any, Literal, assert_never

import pytest

from pyreqwest.client import Client
from pyreqwest.http import Body
from pyreqwest.middleware import Next
from pyreqwest.pytest_plugin.types import MethodMatcher, UrlMatcher, BodyContentMatcher, CustomMatcher, CustomHandler, \
    Matcher, QueryMatcher, SupportsEq
from pyreqwest.request import Request, RequestBuilder
from pyreqwest.response import Response, ResponseBuilder
from pyreqwest.types import Middleware


class Mock:
    def __init__(self, method: MethodMatcher | None = None, path: UrlMatcher | None = None) -> None:
        self._method_matcher = method
        self._path_matcher = path
        self._query_matcher: QueryMatcher | None = None
        self._header_matchers: dict[str, Matcher] = {}
        self._body_matcher: tuple[BodyContentMatcher, Literal["content"]] | tuple[SupportsEq | Literal["json"]] | None = None
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

    def match_query(self, query: QueryMatcher) -> Self:
        self._query_matcher = query
        return self

    def match_query_param(self, name: str, value: Matcher) -> Self:
        if not isinstance(self._query_matcher, dict):
            self._query_matcher = {}
        self._query_matcher[name] = value
        return self

    def match_header(self, name: str, value: Matcher) -> Self:
        self._header_matchers[name] = value
        return self

    def match_body(self, matcher: BodyContentMatcher) -> Self:
        self._body_matcher = (matcher, "content")
        return self

    def match_body_json(self, matcher: SupportsEq) -> Self:
        self._body_matcher = (matcher, "json")
        return self

    def match_request(self, matcher: CustomMatcher) -> Self:
        self._custom_matcher = matcher
        return self

    def match_request_with_response(self, handler: CustomHandler) -> Self:
        assert not self._using_response_builder, "Cannot use response builder and custom handler together"
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
        matched = (
            self._matches_method(request)
            and self._matches_path(request)
            and self._match_query(request)
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
        assert self._custom_handler is None, "Cannot use response builder and custom handler together"
        self._using_response_builder = True
        return ResponseBuilder.create_for_mocking()

    async def _response(self) -> Response:
        if self._built_response is None:
            self._built_response = await self._response_builder.build()
        return self._built_response

    def _matches_method(self, request: Request) -> bool:
        if self._method_matcher is None:
            return True
        elif isinstance(self._method_matcher, set):
            return request.method in self._method_matcher
        else:
            return request.method == self._method_matcher

    def _matches_path(self, request: Request) -> bool:
        path = request.url.with_query_string(None)
        return self._path_matcher is None or self._value_matches(path, self._path_matcher)

    def _match_headers(self, request: Request) -> bool:
        for header_name, expected_value in self._header_matchers.items():
            actual_value = request.headers.get(header_name)
            if actual_value is None or not self._value_matches(actual_value, expected_value):
                return False
        return True

    def _match_body(self, request: Request) -> bool:
        if self._body_matcher is None:
            return True

        if request.body is None:
            return False

        assert request.body.get_stream() is None, "Stream should have been consumed into body bytes by mock middleware"
        body_buf = request.body.copy_bytes()
        assert body_buf is not None, "Unknown body type"
        body_bytes = body_buf.to_bytes()

        matcher, kind = self._body_matcher
        if kind == "json":
            try:
                return json.loads(body_bytes) == matcher
            except json.JSONDecodeError:
                return False
        elif kind == "content":
            if isinstance(matcher, bytes):
                return body_bytes == matcher
            return self._value_matches(body_bytes.decode(), matcher)
        else:
            assert_never(kind)

    def _match_query(self, request: Request) -> bool:
        if self._query_matcher is None:
            return True

        query_str = request.url.query_string or ""
        query_dict = request.url.query_dict_multi_value

        if isinstance(self._query_matcher, dict):
            for key, expected_value in self._query_matcher.items():
                actual_value = query_dict.get(key)
                if actual_value is None or not self._value_matches(actual_value, expected_value):
                    return False
            return True
        else:
            return self._value_matches(query_str, self._query_matcher)

    async def _matches_custom(self, request: Request) -> bool:
        return self._custom_matcher is None or await self._custom_matcher(request)

    def _value_matches(self, value: Any, matcher: Matcher) -> bool:
        if isinstance(matcher, Pattern):
            return matcher.search(str(value)) is not None
        return value == matcher


class ClientMocker:
    """Main class for mocking HTTP requests."""

    def __init__(self) -> None:
        self._mocks: list[Mock] = []
        self._strict = False

    def mock(self, method: MethodMatcher | None = None, path: UrlMatcher | None = None) -> Mock:
        """Add a mock rule for requests matching the given criteria."""
        mock = Mock(method, path)
        self._mocks.append(mock)
        return mock

    def get(self, path: UrlMatcher | None = None) -> Mock:
        """Mock GET requests to the given URL."""
        return self.mock("GET", path)

    def post(self, path: UrlMatcher | None = None) -> Mock:
        """Mock POST requests to the given URL."""
        return self.mock("POST", path)

    def put(self, path: UrlMatcher | None = None) -> Mock:
        """Mock PUT requests to the given URL."""
        return self.mock("PUT", path)

    def patch(self, path: UrlMatcher | None = None) -> Mock:
        """Mock PATCH requests to the given URL."""
        return self.mock("PATCH", path)

    def delete(self, path: UrlMatcher | None = None) -> Mock:
        """Mock DELETE requests to the given URL."""
        return self.mock("DELETE", path)

    def head(self, path: UrlMatcher | None = None) -> Mock:
        """Mock HEAD requests to the given URL."""
        return self.mock("HEAD", path)

    def options(self, path: UrlMatcher | None = None) -> Mock:
        """Mock OPTIONS requests to the given URL."""
        return self.mock("OPTIONS", path)

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
