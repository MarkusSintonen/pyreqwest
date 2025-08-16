from re import Pattern
from typing import Callable, Awaitable, Protocol, Any

from pyreqwest.http import Url
from pyreqwest.request import Request
from pyreqwest.response import Response


class SupportsEq(Protocol):
    def __eq__(self, other: Any) -> bool: ...


Matcher = str | Pattern[str] | SupportsEq

MethodMatcher = str | set[str]
UrlMatcher = Matcher | Url
QueryMatcher = dict[str, Matcher] | Matcher
BodyMatcher = bytes | Matcher
CustomMatcher = Callable[[Request], Awaitable[bool]]
CustomHandler = Callable[[Request], Awaitable[Response | None]]
