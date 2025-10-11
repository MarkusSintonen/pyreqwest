from dataclasses import dataclass
from re import Pattern
from typing import Any

try:
    from dirty_equals import DirtyEquals as _DirtyEqualsBase
except ImportError:
    _DirtyEqualsBase = None  # type: ignore[assignment,misc]


@dataclass
class InternalMatcher:
    matcher: Any
    matcher_repr: str = ""

    def matches(self, value: Any) -> bool:
        if isinstance(self.matcher, Pattern):
            return self.matcher.search(str(value)) is not None
        return bool(value == self.matcher)

    def __post_init__(self) -> None:
        if _DirtyEqualsBase is not None and isinstance(self.matcher, _DirtyEqualsBase):
            # Need to memoize DirtyEquals repr so it is not messing its repr when doing __eq__:
            # https://dirty-equals.helpmanual.io/latest/usage/#__repr__-and-pytest-compatibility
            self.matcher_repr = repr(self.matcher)
        elif isinstance(self.matcher, str):
            self.matcher_repr = self.matcher
        elif isinstance(self.matcher, Pattern):
            self.matcher_repr = f"{self.matcher.pattern} (regex)"
        else:
            self.matcher_repr = repr(self.matcher)

    def __repr__(self) -> str:
        return f"Matcher({self.matcher_repr})"
