# Run via "tests/test_isolated.py" for isolation
import sys
from collections.abc import Generator
from contextlib import contextmanager
from datetime import timedelta

from pyreqwest.client import SyncClientBuilder
from pyreqwest.http import Url
from pyreqwest.runtime import (
    runtime_blocking_thread_keep_alive,
    runtime_max_blocking_threads,
    runtime_multithreaded_default,
    runtime_worker_threads,
)


@contextmanager
def raises(exc: type[BaseException], match: str) -> Generator[None, None, None]:
    try:
        yield
    except BaseException as e:
        if isinstance(e, exc) and match in str(e):
            return
    raise AssertionError(f"Did not raise {exc.__name__} with message containing '{match}'")  # noqa: EM102


def checks() -> None:
    url = Url(sys.argv[1])
    assert sys.argv[2] in ("True", "False")
    set_default = sys.argv[2] == "True"

    runtime_worker_threads(2)  # No effect yet
    runtime_max_blocking_threads(64)
    runtime_blocking_thread_keep_alive(timedelta(seconds=10))

    with SyncClientBuilder().runtime_multithreaded(False).build() as client:
        assert client.get(url).build().send().status == 200
    with SyncClientBuilder().build() as client:  # Default does not use MT
        assert client.get(url).build().send().status == 200

    runtime_worker_threads(3)  # Can still change, MT not used
    runtime_max_blocking_threads(128)
    runtime_blocking_thread_keep_alive(timedelta(seconds=20))

    if set_default:
        runtime_multithreaded_default(True)
        with SyncClientBuilder().build() as client:
            assert client.get(url).build().send().status == 200
    else:
        with SyncClientBuilder().runtime_multithreaded(True).build() as client:
            assert client.get(url).build().send().status == 200

    runtime_worker_threads(3)  # Same value used
    runtime_max_blocking_threads(128)
    runtime_blocking_thread_keep_alive(timedelta(seconds=20))

    msg = "Multi-threaded runtime config can not be changed after the multi-threaded runtime has been initialized"
    with raises(RuntimeError, msg):
        runtime_worker_threads(4)  # Can not change anymore as MT was initialized
    with raises(RuntimeError, msg):
        runtime_max_blocking_threads(256)
    with raises(RuntimeError, msg):
        runtime_blocking_thread_keep_alive(timedelta(seconds=30))

    with SyncClientBuilder().runtime_multithreaded(True).build() as client:
        assert client.get(url).build().send().status == 200

    print("OK")


if __name__ == "__main__":
    checks()
