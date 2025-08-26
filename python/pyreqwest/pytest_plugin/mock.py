import json
from collections.abc import Callable
from functools import cached_property
from re import Pattern
from typing import Any, Literal, Self, assert_never

import pytest

from pyreqwest.client import Client
from pyreqwest.http import Body
from pyreqwest.middleware import Next
from pyreqwest.pytest_plugin.internal.matcher import InternalMatcher
from pyreqwest.pytest_plugin.types import (
    BodyContentMatcher,
    CustomHandler,
    CustomMatcher,
    JsonMatcher,
    Matcher,
    MethodMatcher,
    QueryMatcher,
    UrlMatcher,
)
from pyreqwest.request import Request, RequestBuilder
from pyreqwest.response import Response, ResponseBuilder
from pyreqwest.types import Middleware


class Mock:
    def __init__(self, method: MethodMatcher | None = None, path: UrlMatcher | None = None) -> None:
        self._method_matcher = InternalMatcher(method) if method is not None else None
        self._path_matcher = InternalMatcher(path) if path is not None else None
        self._query_matcher: dict[str, InternalMatcher] | InternalMatcher | None = None
        self._header_matchers: dict[str, InternalMatcher] = {}
        self._body_matcher: tuple[InternalMatcher, Literal["content", "json"]] | None = None
        self._custom_matcher: CustomMatcher | None = None
        self._custom_handler: CustomHandler | None = None

        self._matched_requests: list[Request] = []
        self._unmatched_requests_repr_parts: list[dict[str, str | None]] = []

        self._using_response_builder = False
        self._built_response: Response | None = None

    def assert_called(
        self,
        *,
        count: int | None = None,
        min_count: int | None = None,
        max_count: int | None = None,
    ) -> None:
        """Assert that this mock was called the expected number of times. By default, exactly once."""
        if count is None and min_count is None and max_count is None:
            count = 1

        actual_count = len(self._matched_requests)

        if self._assertion_passes(actual_count, count, min_count, max_count):
            return

        # Generate and raise detailed error message
        from pyreqwest.pytest_plugin.internal.assert_message import assert_fail

        assert_fail(self, count=count, min_count=min_count, max_count=max_count)

    def _assertion_passes(
        self,
        actual_count: int,
        count: int | None,
        min_count: int | None,
        max_count: int | None,
    ) -> bool:
        """Check if the mock call count matches the expected criteria."""
        # Exact count check takes precedence
        if count is not None:
            return actual_count == count

        # Range check (both min and max can be specified independently)
        min_satisfied = min_count is None or actual_count >= min_count
        max_satisfied = max_count is None or actual_count <= max_count

        return min_satisfied and max_satisfied

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
        if isinstance(query, dict):
            self._query_matcher = {k: InternalMatcher(v) for k, v in query.items()}
        else:
            self._query_matcher = InternalMatcher(query)
        return self

    def match_query_param(self, name: str, value: Matcher) -> Self:
        if not isinstance(self._query_matcher, dict):
            self._query_matcher = {}
        self._query_matcher[name] = InternalMatcher(value)
        return self

    def match_header(self, name: str, value: Matcher) -> Self:
        self._header_matchers[name] = InternalMatcher(value)
        return self

    def match_body(self, matcher: BodyContentMatcher) -> Self:
        self._body_matcher = (InternalMatcher(matcher), "content")
        return self

    def match_body_json(self, matcher: JsonMatcher) -> Self:
        self._body_matcher = (InternalMatcher(matcher), "json")
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
        matches = {
            "method": self._matches_method(request),
            "path": self._matches_path(request),
            "query": self._match_query(request),
            "headers": self._match_headers(request),
            "body": self._match_body(request),
            "custom": await self._matches_custom(request),
            "handler": True,
        }
        if all(matches.values()):
            response = (
                await self._custom_handler(request) if self._custom_handler is not None else await self._response()
            )
            matches["handler"] = self._custom_handler is None or response is not None
        else:
            response = None

        if response is not None:
            self._matched_requests.append(request)
            return response
        from pyreqwest.pytest_plugin.internal.assert_message import format_unmatched_request_parts

        # Memo the reprs as we may consume the request
        self._unmatched_requests_repr_parts.append(
            format_unmatched_request_parts(request, unmatched={k for k, matched in matches.items() if not matched}),
        )
        return None

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
        return self._method_matcher is None or self._method_matcher.matches(request.method)

    def _matches_path(self, request: Request) -> bool:
        return self._path_matcher is None or self._path_matcher.matches(request.url.path)

    def _match_headers(self, request: Request) -> bool:
        for header_name, expected_value in self._header_matchers.items():
            actual_value = request.headers.get(header_name)
            if actual_value is None or not expected_value.matches(actual_value):
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
                return matcher.matches(json.loads(body_bytes))
            except json.JSONDecodeError:
                return False
        elif kind == "content":
            if isinstance(matcher.matcher, bytes):
                return matcher.matches(body_bytes)
            return matcher.matches(body_bytes.decode())
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
                if actual_value is None or not expected_value.matches(actual_value):
                    return False
            return True
        if isinstance(self._query_matcher.matcher, str | Pattern):
            return self._query_matcher.matches(query_str)
        return self._query_matcher.matches(query_dict)

    async def _matches_custom(self, request: Request) -> bool:
        return self._custom_matcher is None or await self._custom_matcher(request)


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
                msg = f"No mock rule matched request: {request.method} {request.url}"
                raise AssertionError(msg)
            return await next_handler.run(request)  # Proceed normally

        return mock_middleware


@pytest.fixture
def client_mocker(monkeypatch: pytest.MonkeyPatch) -> ClientMocker:
    mocker = ClientMocker()

    orig_build_consumed = RequestBuilder.build_consumed
    orig_build_streamed = RequestBuilder.build_streamed

    def build_patch(self: RequestBuilder, orig: Callable) -> Request:
        request = orig(self)
        assert request._interceptor is None
        request._interceptor = mocker._create_middleware()
        return request

    monkeypatch.setattr(RequestBuilder, "build_consumed", lambda slf: build_patch(slf, orig_build_consumed))
    monkeypatch.setattr(RequestBuilder, "build_streamed", lambda slf: build_patch(slf, orig_build_streamed))
    return mocker
