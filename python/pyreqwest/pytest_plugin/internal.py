import json
import re
from typing import assert_never, Literal

from pyreqwest.pytest_plugin import Mock
from pyreqwest.pytest_plugin.types import (
    MethodMatcher,
    UrlMatcher,
    QueryMatcher,
    Matcher,
    BodyContentMatcher,
    JsonMatcher
)


def format_assert_called_error(
    mock: Mock,
    *,
    count: int | None = None,
    min_count: int | None = None,
    max_count: int | None = None
) -> str:
    actual_count = len(mock._matched_requests)
    error_parts = ["Mock was not called as expected."]

    # Add expected vs actual count information
    if count is not None:
        error_parts.append(f"Expected exactly {count} call(s), but got {actual_count}.")
    else:
        expectations = []
        if min_count is not None:
            expectations.append(f"at least {min_count}")
        if max_count is not None:
            expectations.append(f"at most {max_count}")
        expected_desc = " and ".join(expectations)
        error_parts.append(f"Expected {expected_desc} call(s), but got {actual_count}.")

    error_parts.append("\nMock configuration:")
    error_parts.append(_format_mock_matchers(mock))

    if mock._unmatched_requests_repr:
        error_parts.append(f"\nUnmatched requests ({len(mock._unmatched_requests_repr)}):")
        for i, request_repr in enumerate(mock._unmatched_requests_repr[-5:], 1):
            error_parts.append(f"  {i}. {request_repr}")
        if len(mock._unmatched_requests_repr) > 5:
            error_parts.append(f"  ... and {len(mock._unmatched_requests_repr) - 5} more")

    if mock._matched_requests:
        error_parts.append(f"\nMatched requests ({len(mock._matched_requests)}):")
        for i, request in enumerate(mock._matched_requests[-3:], 1):
            error_parts.append(f"  {i}. {request.repr_full()}")
        if len(mock._matched_requests) > 3:
            error_parts.append(f"  ... and {len(mock._matched_requests) - 3} more")

    return "\n".join(error_parts)


def _format_mock_matchers(mock: Mock) -> str:
    parts = [
        _format_method_matcher(mock._method_matcher),
        _format_path_matcher(mock._path_matcher),
    ]

    if mock._query_matcher is not None:
        parts.append(_format_query_matcher(mock._query_matcher))

    if mock._header_matchers:
        parts.append(_format_header_matchers(mock._header_matchers))

    if mock._body_matcher is not None:
        parts.append(_format_body_matcher(*mock._body_matcher))

    if mock._custom_matcher is not None:
        parts.append(f"  Custom matcher: {mock._custom_matcher.__name__}")

    if mock._custom_handler is not None:
        parts.append(f"  Custom handler: {mock._custom_handler.__name__}")

    return "\n".join(parts) if parts else "  No specific matchers"


def _format_method_matcher(method_matcher: MethodMatcher | None) -> str:
    if method_matcher is None:
        return "  Method: Any"
    elif isinstance(method_matcher, set):
        return f"  Method: {' or '.join(sorted(method_matcher))}"
    else:
        return f"  Method: {method_matcher}"


def _format_path_matcher(path_matcher: UrlMatcher | None) -> str:
    if path_matcher is None:
        return "  Path: Any"
    elif isinstance(path_matcher, re.Pattern):
        return f"  Path: {path_matcher.pattern} (regex)"
    else:
        return f"  Path: {path_matcher}"


def _format_query_matcher(query_matcher: QueryMatcher) -> str:
    if isinstance(query_matcher, dict):
        query_parts = [f"{k}={v}" for k, v in query_matcher.items()]
        return f"  Query: {', '.join(query_parts)}"
    elif isinstance(query_matcher, re.Pattern):
        return f"  Query: {query_matcher.pattern} (regex)"
    else:
        return f"  Query: {query_matcher}"


def _format_header_matchers(header_matchers: dict[str, Matcher]) -> str:
    header_parts = []
    for name, value in header_matchers.items():
        if isinstance(value, re.Pattern):
            header_parts.append(f"{name}: {value.pattern} (regex)")
        else:
            header_parts.append(f"{name}: {value}")
    return f"  Headers: {', '.join(header_parts)}"


def _format_body_matcher(matcher: BodyContentMatcher | JsonMatcher, kind: Literal["content", "json"]) -> str:
    if kind == "json":
        return f"  Body (JSON): {json.dumps(matcher, separators=(',', ':'))}"
    elif kind == "content":
        if isinstance(matcher, bytes):
            return f"  Body (bytes): {matcher!r}"
        elif isinstance(matcher, re.Pattern):
            return f"  Body (text): {matcher.pattern} (regex)"
        else:
            return f"  Body (text): {matcher!r}"
    else:
        assert_never(kind)
