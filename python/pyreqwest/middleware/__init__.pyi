from pyreqwest.request import Request
from pyreqwest.response import Response, SyncResponse

class Next:
    """Next middleware caller in the chain."""
    async def run(self, request: Request) -> Response:
        """Call the next middleware in the chain with the given request."""

class SyncNext:
    """Next middleware caller in the chain."""
    def run(self, request: Request) -> SyncResponse:
        """Call the next middleware in the chain with the given request."""
