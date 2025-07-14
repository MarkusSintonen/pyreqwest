from typing import Protocol

from pyreqwest.request import Request
from pyreqwest.response import Response


class Middleware(Protocol):
    async def handle(self, request: Request, next: Next) -> Response: ...


class Next:
    async def run(self, request: Request) -> Response: ...
