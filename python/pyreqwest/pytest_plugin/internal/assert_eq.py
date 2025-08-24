from typing import Any


# pytest register_assert_rewrite for pretty diffs
def assert_eq(actual: Any, expected: Any, msg: str) -> None:
    assert actual == expected, msg
