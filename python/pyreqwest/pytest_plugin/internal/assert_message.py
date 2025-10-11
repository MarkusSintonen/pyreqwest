import json
from typing import Literal, assert_never

from pyreqwest.pytest_plugin import Mock
from pyreqwest.pytest_plugin.internal.assert_eq import assert_eq
from pyreqwest.pytest_plugin.internal.matcher import InternalMatcher
from pyreqwest.request import Request


def assert_fail(
    mock: Mock,
    *,
    count: int | None = None,
    min_count: int | None = None,
    max_count: int | None = None,
) -> None:
    msg = _format_counts_assert_message(mock, count, min_count, max_count)

    if mock._unmatched_requests_repr_parts:
        not_matched = {*mock._unmatched_requests_repr_parts[-1].keys()}
        assert not_matched

        msg = f"{msg}. Diff with last unmatched request:"
        assert_eq(mock._unmatched_requests_repr_parts[-1], _format_mock_matchers_parts(mock, not_matched), msg)
    else:
        raise AssertionError(msg)


def _format_counts_assert_message(
    mock: Mock,
    count: int | None = None,
    min_count: int | None = None,
    max_count: int | None = None,
) -> str:
    if count is not None:
        expected_desc = f"exactly {count}"
    else:
        expectations = []
        if min_count is not None:
            expectations.append(f"at least {min_count}")
        if max_count is not None:
            expectations.append(f"at most {max_count}")
        expected_desc = " and ".join(expectations)

    method_path = " ".join(
        [
            mock._method_matcher.matcher_repr if mock._method_matcher is not None else "*",
            mock._path_matcher.matcher_repr if mock._path_matcher is not None else "*",
        ],
    )
    return f'Expected {expected_desc} request(s) but received {len(mock._matched_requests)} to: "{method_path}"'


def format_unmatched_request_parts(request: Request, unmatched: set[str]) -> dict[str, str | None]:
    req_parts: dict[str, str | None] = {
        "method": request.method,
        "url": str(request.url),
        "path": request.url.path,
        "query": None,
        "headers": None,
        "body": None,
    }

    if request.url.query_pairs:
        query_parts = [f"{k}={v}" for k, v in request.url.query_pairs]
        req_parts["query"] = ", ".join(query_parts)

    if request.headers:
        header_parts = [f"{name.title()}: {value}" for name, value in request.headers.items()]
        req_parts["headers"] = ", ".join(header_parts)

    if request.body:
        if (bytes_body := request.body.copy_bytes()) is not None:
            req_parts["body"] = bytes_body.to_bytes().decode("utf8", errors="replace")
        elif (stream_body := request.body.get_stream()) is not None:
            req_parts["body"] = repr(stream_body)
        else:
            req_parts["body"] = repr(request.body)

    fmt_parts: dict[str, str | None] = {
        "custom": f"No match with request {req_parts}",
        "handler": f"No match with request {req_parts}",
    }
    fmt_parts = {**req_parts, **fmt_parts}

    return {k: v for k, v in fmt_parts.items() if k in unmatched}


def _format_mock_matchers_parts(mock: Mock, unmatched: set[str] | None) -> dict[str, str | None]:
    parts: dict[str, str | None] = {
        "method": mock._method_matcher.matcher_repr if mock._method_matcher is not None else None,
        "path": mock._path_matcher.matcher_repr if mock._path_matcher is not None else None,
        "query": _format_query_matcher(mock._query_matcher) if mock._query_matcher is not None else None,
        "headers": _format_header_matchers(mock._header_matchers) if mock._header_matchers is not None else None,
        "body": _format_body_matcher(*mock._body_matcher) if mock._body_matcher is not None else None,
        "custom": f"Custom matcher: {mock._custom_matcher.__name__}" if mock._custom_matcher is not None else None,
        "handler": f"Custom handler: {mock._custom_handler.__name__}" if mock._custom_handler is not None else None,
    }
    return {k: v for k, v in parts.items() if unmatched is None or k in unmatched}


def _format_query_matcher(query_matcher: dict[str, InternalMatcher] | InternalMatcher) -> str:
    if isinstance(query_matcher, dict):
        query_parts = [f"{k}={v.matcher_repr}" for k, v in query_matcher.items()]
        return ", ".join(query_parts)
    return query_matcher.matcher_repr


def _format_header_matchers(header_matchers: dict[str, InternalMatcher]) -> str:
    header_parts = [f"{name.title()}: {value.matcher_repr}" for name, value in header_matchers.items()]
    return ", ".join(header_parts)


def _format_body_matcher(matcher: InternalMatcher, kind: Literal["content", "json"]) -> str:
    if kind == "json":
        try:
            return json.dumps(matcher.matcher, separators=(",", ":"))
        except (TypeError, ValueError):
            return matcher.matcher_repr
    elif kind == "content":
        return matcher.matcher_repr
    else:
        assert_never(kind)
