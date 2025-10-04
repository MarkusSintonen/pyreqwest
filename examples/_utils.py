import base64
import inspect
import os
from collections.abc import Awaitable, Callable
from types import ModuleType

from pyreqwest.http import Url

CallableExample = Callable[[], Awaitable[None]] | Callable[[], None]


def httpbin_url() -> Url:
    return Url(os.environ.get("HTTPBIN", "https://httpbin.org"))


async def run_examples(mod: ModuleType) -> None:
    """Runner"""
    for fn in collect_examples(mod):
        await run_example(fn)


async def run_example(fn: CallableExample) -> None:
    """Runner"""
    print(f"\n# running: {fn.__name__}")
    if inspect.iscoroutinefunction(fn):
        await fn()
    else:
        fn()


def collect_examples(mod: ModuleType) -> list[CallableExample]:
    """Collect example functions from a module"""
    return sorted(
        (obj for name, obj in inspect.getmembers(mod) if name.startswith("example_")),
        key=lambda f: f.__code__.co_firstlineno,
    )


def parse_data_uri(body: str) -> str:
    if body.startswith("data:"):
        return base64.b64decode(body.split(",", maxsplit=1)[1]).decode()
    return body
