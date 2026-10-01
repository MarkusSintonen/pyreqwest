import os
import sys
import sysconfig

import pytest


@pytest.mark.skipif(not os.environ.get("PYREQWEST_TEST_FREE_THREADED"), reason="Free-threaded check not enabled")
def test_gil_disabled() -> None:
    import pyreqwest  # noqa: F401

    assert sysconfig.get_config_var("Py_GIL_DISABLED") == 1
    assert sys.version_info >= (3, 13)
    assert not sys._is_gil_enabled()
