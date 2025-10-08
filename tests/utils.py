import os
import platform
from collections.abc import Generator
from contextlib import contextmanager
from pathlib import Path
from tempfile import NamedTemporaryFile

IS_CI = os.environ.get("CI") is not None
IS_OSX = platform.system() == "Darwin"
IS_WINDOWS = platform.system() == "Windows"


@contextmanager
def temp_file(content: bytes, suffix: str = "") -> Generator[Path, None, None]:
    """Temp file that works on windows too with subprocesses."""
    tmp = NamedTemporaryFile(suffix=suffix, delete=False)  # noqa: SIM115
    path = Path(tmp.name)
    try:
        tmp.write(content)
        tmp.flush()
        tmp.close()
        yield path
    finally:
        path.unlink()
