from pyreqwest.request import Request
from pyreqwest.response import Response, ResponseBuilder


class Next:
    async def run(self, request: Request) -> Response: ...
    def override_response_builder(self) -> ResponseBuilder: ...
