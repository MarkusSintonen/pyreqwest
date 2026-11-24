# Run via "tests/test_isolated.py" for isolation: pyreqwest must be first imported in a short-lived thread
# https://github.com/MarkusSintonen/pyreqwest/issues/59
import threading
from collections.abc import Callable


def run_in_thread(target: Callable[[], None]) -> None:
    thread = threading.Thread(target=target)
    thread.start()
    thread.join()


def import_client() -> None:
    import pyreqwest.client  # noqa: F401


def use_client() -> None:
    from pyreqwest.client import SyncClientBuilder

    SyncClientBuilder().build().close()


def checks() -> None:
    run_in_thread(import_client)
    run_in_thread(use_client)
    print("OK")


if __name__ == "__main__":
    checks()
