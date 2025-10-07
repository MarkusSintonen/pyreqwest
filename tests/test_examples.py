import importlib
import time
from collections.abc import Generator
from pathlib import Path

import docker
import pytest
from docker.models.containers import Container
from pyreqwest.http import Url
from syrupy import SnapshotAssertion  # type: ignore[attr-defined]

from examples._utils import CallableExample, collect_examples, run_example
from tests.utils import IS_CI, IS_OSX, IS_WINDOWS

EXAMPLE_FUNCS: list[tuple[str, CallableExample]] = [
    (p.stem, func)
    for p in (Path(__file__).parent.parent / "examples").iterdir()
    if p.suffix == ".py" and not p.name.startswith("_")
    for func in collect_examples(importlib.import_module(f"examples.{p.stem}"))
]
HTTPBIN_CONTAINER = "httpbin-test-runner"


@pytest.fixture(scope="session")
def httpbin() -> Generator[Url, None, None]:
    # Start Go httpbin server in docker
    client = docker.from_env()

    for container in client.containers.list(filters={"name": HTTPBIN_CONTAINER}, all=True):
        container.remove(v=True, force=True)  # Remove existing

    container = client.containers.run(
        "ghcr.io/mccutchen/go-httpbin:2.18.3@sha256:3992f3763e9ce5a4307eae0a869a78b4df3931dc8feba74ab823dd2444af6a6b",
        name=HTTPBIN_CONTAINER,
        ports={"8080/tcp": None},
        detach=True,
        remove=True,
    )
    assert isinstance(container, Container)

    deadline = time.time() + 10
    host_port = None

    while time.time() < deadline and not host_port:
        container.reload()
        host_port = container.ports.get("8080/tcp", [{}])[0].get("HostPort")
        if not host_port:
            time.sleep(0.1)

    assert host_port, "container did not start in time"
    try:
        yield Url(f"http://localhost:{host_port}")
    finally:
        container.remove(v=True, force=True)


@pytest.mark.parametrize(("module", "func"), EXAMPLE_FUNCS)
@pytest.mark.skipif(IS_CI and (IS_OSX or IS_WINDOWS), reason="No docker setup in CI for OSX/Windows")
async def test_examples(
    capsys: pytest.CaptureFixture[str],
    httpbin: Url,
    snapshot: SnapshotAssertion,
    monkeypatch: pytest.MonkeyPatch,
    module: str,
    func: CallableExample,
) -> None:
    monkeypatch.setenv("HTTPBIN", str(httpbin))

    await run_example(func)

    normalized = capsys.readouterr().out.replace(f":{httpbin.port}/", ":<PORT>/")
    assert normalized == snapshot
