from typing import AsyncGenerator, Sequence, Mapping, Any

from pyreqwest.http import Url

UrlType = Url | str
Stream = AsyncGenerator[bytes | bytearray | memoryview]
Params = Sequence[tuple[str, Any]] | Mapping[str, Any]
