from typing import AsyncGenerator

from pyreqwest.http import Url

UrlType = Url | str
Stream = AsyncGenerator[bytes | bytearray | memoryview]
