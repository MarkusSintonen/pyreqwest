"""Tests that need a fresh interpreter. Scripts in "tests/isolated" print "OK" on success."""

import subprocess
import sys
from pathlib import Path

import pytest

from tests.servers.server_subprocess import SubprocessServer

ISOLATED_DIR = Path(__file__).parent / "isolated"


def run_isolated(script: str, *args: str) -> None:
    process = subprocess.run(  # noqa: S603
        [sys.executable, str(ISOLATED_DIR / script), *args],
        capture_output=True,
        text=True,
        timeout=20,
        check=False,
    )
    assert process.returncode == 0, f"exit code {process.returncode}: {process.stderr}"
    assert process.stdout.strip() == "OK"


@pytest.mark.parametrize("set_default", [False, True])
def test_runtime_config(echo_server: SubprocessServer, set_default: bool) -> None:
    run_isolated("runtime_config.py", str(echo_server.url), str(set_default))


def test_import_and_use_in_exited_threads() -> None:
    # https://github.com/MarkusSintonen/pyreqwest/issues/59
    run_isolated("allocator_thread_exit.py")
