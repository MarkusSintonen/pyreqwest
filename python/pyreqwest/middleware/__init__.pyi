from pyreqwest.request import Request
from pyreqwest.response import Response, SyncResponse

class Next:
    async def run(self, request: Request) -> Response: ...

class SyncNext:
    def run(self, request: Request) -> SyncResponse: ...
