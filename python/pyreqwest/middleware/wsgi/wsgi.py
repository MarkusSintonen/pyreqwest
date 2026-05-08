"""WSGI middleware."""

import contextlib
import io
from collections.abc import Callable, Iterable
from typing import TYPE_CHECKING, Any, cast
from urllib.parse import unquote

from pyreqwest.middleware import SyncNext
from pyreqwest.request import Request
from pyreqwest.response import ResponseBuilder, SyncResponse

if TYPE_CHECKING:
    from pyreqwest.types import SyncStream

StartResponse = Callable[[str, list[tuple[str, str]], Any | None], Callable[[bytes], None]]
WSGIApp = Callable[[dict[str, Any], StartResponse], Iterable[bytes]]


class WSGITestMiddleware:
    """Test client that routes requests into a WSGI application."""

    def __init__(
        self,
        app: WSGIApp,
        *,
        scope_update: Callable[[dict[str, Any], Request], None] | None = None,
    ) -> None:
        """Initialize the WSGI test client.

        Args:
            app: WSGI application callable
            scope_update: Optional callable to modify the WSGI environ per request
        """
        self._app = app
        self._scope_update = scope_update

    def __call__(self, request: Request, _next_handler: SyncNext) -> SyncResponse:
        """WSGI middleware handler."""
        environ = self._request_to_wsgi_environ(request)

        response_builder = ResponseBuilder()
        headers_set: bool = False

        def start_response(
            status: str,
            response_headers: list[tuple[str, str]],
            exc_info: Any | None = None,
        ) -> Callable[[bytes], None]:
            nonlocal headers_set
            if exc_info:
                try:
                    if headers_set:
                        # Re-raise original exception if headers have already been sent
                        raise exc_info[1].with_traceback(exc_info[2])
                finally:
                    exc_info = None  # avoid dangling circular ref

            status_code = int(status.split(" ", 1)[0])
            response_builder.status(status_code)
            response_builder.headers(response_headers)
            headers_set = True

            def write(data: bytes) -> None:
                msg = "WSGI write callable is not supported by this test client. Yield bytes from the app instead."
                raise NotImplementedError(msg)

            return write

        body_iterable = self._app(environ, start_response)

        iterator = iter(body_iterable)
        first_chunk = None
        with contextlib.suppress(StopIteration):
            first_chunk = next(iterator)

        if not headers_set:
            msg = "WSGI app returned without calling start_response"
            raise RuntimeError(msg)

        def stream_wrapper() -> Iterable[bytes]:
            if first_chunk is not None:
                yield first_chunk
            yield from iterator

        response_builder.body_stream(stream_wrapper())
        return response_builder.build_sync()

    def _request_to_wsgi_environ(self, request: Request) -> dict[str, Any]:
        url = request.url
        environ: dict[str, Any] = {
            "REQUEST_METHOD": request.method.upper(),
            "SCRIPT_NAME": "",
            "PATH_INFO": unquote(url.path),
            "QUERY_STRING": url.query_string or "",
            "SERVER_NAME": url.host_str or "localhost",
            "SERVER_PORT": str(url.port or (443 if url.scheme == "https" else 80)),
            "SERVER_PROTOCOL": "HTTP/1.1",
            "wsgi.version": (1, 0),
            "wsgi.url_scheme": url.scheme or "http",
            "wsgi.input": self._wsgi_input(request),
            "wsgi.errors": io.StringIO(),
            "wsgi.multithread": False,
            "wsgi.multiprocess": False,
            "wsgi.run_once": False,
        }

        for name, value in request.headers.items():
            key = f"HTTP_{name.upper().replace('-', '_')}"
            if key not in environ:
                environ[key] = value
            else:
                environ[key] = f"{environ[key]},{value}"

        if "HTTP_CONTENT_TYPE" in environ:
            environ["CONTENT_TYPE"] = environ.pop("HTTP_CONTENT_TYPE")
        if "HTTP_CONTENT_LENGTH" in environ:
            environ["CONTENT_LENGTH"] = environ.pop("HTTP_CONTENT_LENGTH")

        if self._scope_update is not None:
            self._scope_update(environ, request)

        return environ

    def _wsgi_input(self, request: Request) -> io.BytesIO:
        body = request.body
        if body is None:
            return io.BytesIO(b"")

        if (bytes_body := body.copy_bytes()) is not None:
            return io.BytesIO(bytes_body.to_bytes())

        stream = body.get_stream()
        if stream is not None:
            sync_stream: SyncStream = stream  # type: ignore[assignment]  # WSGI is always sync
            return io.BytesIO(b"".join(cast("Iterable[bytes]", sync_stream)))

        return io.BytesIO(b"")
