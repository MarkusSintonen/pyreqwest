from typing import AsyncIterable, Sequence, Mapping, Any, Protocol

from pyreqwest.client import Client
from pyreqwest.http import Url
from pyreqwest.middleware import Next
from pyreqwest.request import Request
from pyreqwest.response import Response

UrlType = Url | str
HeadersType = Mapping[str, str] | Sequence[tuple[str, str]]
QueryParams = Mapping[str, Any] | Sequence[tuple[str, Any]]
FormParams = Mapping[str, Any] | Sequence[tuple[str, Any]]
ExtensionsType = Mapping[str, Any] | Sequence[tuple[str, Any]]
Stream = AsyncIterable[bytes] | AsyncIterable[bytearray] | AsyncIterable[memoryview]


class Middleware(Protocol):
    async def __call__(self, client: Client, request: Request, next_handler: Next) -> Response: ...
