from typing import AsyncGenerator, Sequence, Mapping, Any, Protocol

from pyreqwest.client import Client
from pyreqwest.http import Url
from pyreqwest.middleware import Next
from pyreqwest.request import Request
from pyreqwest.response import Response

UrlType = Url | str
Stream = AsyncGenerator[bytes] | AsyncGenerator[bytearray] | AsyncGenerator[memoryview]
Params = Sequence[tuple[str, Any]] | Mapping[str, Any]


class Middleware(Protocol):
    async def __call__(self, client: Client, request: Request, next_: Next) -> Response: ...
