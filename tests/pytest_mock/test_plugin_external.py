from typing import TYPE_CHECKING

from pyreqwest.client import ClientBuilder

if TYPE_CHECKING:
    # Do not load at runtime to check that the fixture is available through pytest plugin loading
    from pyreqwest.pytest_plugin import ClientMocker


async def test_fixture_loads(client_mocker: "ClientMocker") -> None:
    """This test should run in isolation from other tests to verify that the
    client_mocker fixture is available through pytest's plugin discovery
    mechanism, not through direct imports in other test files.
    """
    client_mocker.get(path="/").with_body_text("test")

    resp = await ClientBuilder().build().get("http://example.invalid").build().send()
    assert resp.status == 200
    assert await resp.text() == "test"
