from pyreqwest.request import Request
from pyreqwest.response import Response


class Next:
    async def run(self, request: Request) -> Response: ...
