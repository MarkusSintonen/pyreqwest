import inspect
import os
from collections.abc import Awaitable, Callable
from types import ModuleType

from pyreqwest.http import Url

HTTPBIN = Url(os.environ.get("HTTPBIN", "https://httpbin.org"))


async def run_examples(mod: ModuleType) -> None:
    """Runner"""
    for fn in _collect_examples(mod):
        print(f"\n# running: {fn.__name__}")
        if inspect.iscoroutinefunction(fn):
            await fn()
        else:
            fn()


def _collect_examples(mod: ModuleType) -> list[Callable[[], Awaitable[None]] | Callable[[], None]]:
    """Collect example functions from a module"""
    return sorted(
        (obj for name, obj in inspect.getmembers(mod) if name.startswith("example_")),
        key=lambda f: f.__code__.co_firstlineno,
    )
