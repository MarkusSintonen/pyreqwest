import json

from pyreqwest.pytest_plugin import Mock


def format_assert_called_error(
    mock: Mock,
    actual_count: int,
    *,
    count: int | None = None,
    min_count: int | None = None,
    max_count: int | None = None
) -> str:
    error_parts = ["Mock was not called as expected."]

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
    parts = []

    if mock._method_matcher is not None:
        if isinstance(mock._method_matcher, set):
            parts.append(f"  Method: {' or '.join(sorted(mock._method_matcher))}")
        else:
            parts.append(f"  Method: {mock._method_matcher}")
    else:
        parts.append("  Method: Any")

    if mock._path_matcher is not None:
        if hasattr(mock._path_matcher, 'pattern'):
            parts.append(f"  Path: {mock._path_matcher.pattern} (regex)")
        else:
            parts.append(f"  Path: {mock._path_matcher}")
    else:
        parts.append("  Path: Any")

    if mock._query_matcher is not None:
        if isinstance(mock._query_matcher, dict):
            query_parts = [f"{k}={v}" for k, v in mock._query_matcher.items()]
            parts.append(f"  Query: {', '.join(query_parts)}")
        elif hasattr(mock._query_matcher, 'pattern'):
            parts.append(f"  Query: {mock._query_matcher.pattern} (regex)")
        else:
            parts.append(f"  Query: {mock._query_matcher}")

    if mock._header_matchers:
        header_parts = []
        for name, value in mock._header_matchers.items():
            if hasattr(value, 'pattern'):
                header_parts.append(f"{name}: {value.pattern} (regex)")
            else:
                header_parts.append(f"{name}: {value}")
        parts.append(f"  Headers: {', '.join(header_parts)}")

    if mock._body_matcher is not None:
        matcher, kind = mock._body_matcher
        if kind == "json":
            parts.append(f"  Body (JSON): {json.dumps(matcher, separators=(',', ':'))}")
        elif kind == "content":
            if isinstance(matcher, bytes):
                parts.append(f"  Body (bytes): {matcher!r}")
            elif hasattr(matcher, 'pattern'):
                parts.append(f"  Body (text): {matcher.pattern} (regex)")
            else:
                parts.append(f"  Body (text): {matcher!r}")

    if mock._custom_matcher is not None:
        if hasattr(mock._custom_matcher, '__name__'):
            parts.append(f"  Custom matcher: {mock._custom_matcher.__name__}")
        else:
            parts.append(f"  Custom matcher: {type(mock._custom_matcher).__name__}")

    if mock._custom_handler is not None:
        if hasattr(mock._custom_handler, '__name__'):
            parts.append(f"  Custom handler: {mock._custom_handler.__name__}")
        else:
            parts.append(f"  Custom handler: {type(mock._custom_handler).__name__}")

    return "\n".join(parts) if parts else "  No specific matchers"
