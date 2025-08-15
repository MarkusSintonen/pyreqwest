"""PyReqwest pytest plugin entry point."""

from .mock import client_mocker  # load the client_mocker fixture


def pytest_configure(config):
    """Configure the pytest plugin."""
    config.addinivalue_line(
        "markers",
        "pyreqwest: mark test to use PyReqwest HTTP client mocking"
    )
