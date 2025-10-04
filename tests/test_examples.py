import importlib
from pathlib import Path
from typing import Any

import pytest
from pyreqwest.http import Url
from syrupy import SnapshotAssertion  # type: ignore[attr-defined]

from examples._utils import run_examples

EXAMPLE_MODULES = [
    p.stem
    for p in (Path(__file__).parent.parent / "examples").iterdir()
    if p.suffix == ".py" and not p.name.startswith("_")
]


@pytest.mark.parametrize("example", EXAMPLE_MODULES)
async def test_examples(
    capsys: pytest.CaptureFixture[str],
    httpbin: Any,
    snapshot: SnapshotAssertion,
    monkeypatch: pytest.MonkeyPatch,
    example: str,
) -> None:
    url = Url(httpbin.url)
    monkeypatch.setenv("HTTPBIN", str(url))

    await run_examples(importlib.import_module(f"examples.{example}"))

    normalized = capsys.readouterr().out.replace(f":{url.port}/", ":<PORT>/")
    assert normalized == snapshot
