from datetime import timedelta
from typing import Any, Self

from pyreqwest.bytes import Bytes
from pyreqwest.http import HeaderMap, Url
from pyreqwest.middleware.types import Middleware, SyncMiddleware
from pyreqwest.multipart import FormBuilder
from pyreqwest.response import Response, SyncResponse
from pyreqwest.types import ExtensionsType, FormParams, HeadersType, QueryParams, Stream, SyncStream

class Request:
    @property
    def method(self) -> str:
        """Get the HTTP method. (e.g. GET, POST)."""

    @method.setter
    def method(self, value: str) -> None:
        """Set the HTTP method."""

    @property
    def url(self) -> Url:
        """Get the url."""

    @url.setter
    def url(self, value: Url | str) -> None:
        """Set the url."""

    @property
    def headers(self) -> HeaderMap:
        """Get the headers. This is not a copy. Modifying it modifies the request."""

    @headers.setter
    def headers(self, headers: HeadersType) -> None:
        """Replace headers. Given value is copied."""

    @property
    def body(self) -> "RequestBody | None":
        """Get the body."""

    @body.setter
    def body(self, value: "RequestBody | None") -> None:
        """Set the body or remove body."""

    @property
    def extensions(self) -> dict[str, Any]:
        """Arbitrary per-request data storage. Useful for passing through data to middleware and response."""

    @extensions.setter
    def extensions(self, value: ExtensionsType) -> None:
        """Replace extensions. Given value is shallow copied."""

    def copy(self) -> Self:
        """Copy the request. Byte-bodies are zero-copied. Stream bodies are re-created via their own copy logic."""

    def __copy__(self) -> Self: ...
    def repr_full(self) -> str:
        """Verbose repr including non-sensitive headers and body summary."""

    @classmethod
    def from_request_and_body(cls, request: Self, body: "RequestBody | None") -> Self:
        """Clone request with a new body instance."""

class ConsumedRequest(Request):
    """Request that will fully read the response body when sent."""

    async def send(self) -> Response:
        """Execute the request returning a Response with fully read response body."""

class StreamRequest(Request):
    """Request whose response body is streamed."""

    async def __aenter__(self) -> Response:
        """Execute the request returning a Response with streaming response body."""

    async def __aexit__(self, *args: object, **kwargs: Any) -> None:
        """Close streaming response."""

    @property
    def read_buffer_limit(self) -> int:
        """Max bytes buffered when reading streamed body."""

class SyncConsumedRequest(Request):
    """Synchronous request that will fully read the response body when sent."""

    def send(self) -> SyncResponse:
        """Execute the request returning a Response with fully read response body."""

class SyncStreamRequest(Request):
    """Synchronous request whose response body is streamed."""

    def __enter__(self) -> SyncResponse:
        """Execute the request returning a Response with streaming response body."""

    def __exit__(self, *args: object, **kwargs: Any) -> None:
        """Close streaming response."""

    @property
    def read_buffer_limit(self) -> int:
        """Max bytes buffered when reading streamed body."""

class RequestBody:
    """Represents request body content (bytes, text, or async stream). Bodies are single-use."""

    @staticmethod
    def from_text(body: str) -> "RequestBody":
        """Create body from text."""

    @staticmethod
    def from_bytes(body: bytes | bytearray | memoryview) -> "RequestBody":
        """Create body from raw bytes."""

    @staticmethod
    def from_stream(stream: Stream) -> "RequestBody":
        """Create body from async byte stream."""

    def copy_bytes(self) -> Bytes | None:
        """Return bytes zero-copy. None for stream."""

    def get_stream(self) -> Stream | None:
        """Return underlying stream if streaming body else None."""

    def __copy__(self) -> Self:
        """Copy body (Zero-copied bytes. Stream supplies its own copy)."""

class BaseRequestBuilder:
    def error_for_status(self, enable: bool) -> Self:
        """Enable automatic HTTP error raising (4xx/5xx)."""

    def header(self, name: str, value: str) -> Self:
        """Append single header value."""

    def headers(self, headers: HeadersType) -> Self:
        """Merge multiple headers (mapping or sequence)."""

    def basic_auth(self, username: str, password: str | None) -> Self:
        """Add Basic Authorization header."""

    def bearer_auth(self, token: str) -> Self:
        """Add Bearer token Authorization header."""

    def body_bytes(self, body: bytes | bytearray | memoryview) -> Self:
        """Set body from raw bytes."""

    def body_text(self, body: str) -> Self:
        """Set body from text."""

    def body_json(self, body: Any) -> Self:
        """Serialize body as JSON. Sets Content-Type header."""

    def query(self, query: QueryParams) -> Self:
        """Add/merge query parameters."""

    def timeout(self, timeout: timedelta) -> Self:
        """Set per-request total timeout."""

    def multipart(self, multipart: FormBuilder) -> Self:
        """Attach multipart form body builder."""

    def form(self, form: FormParams) -> Self:
        """Set application/x-www-form-urlencoded body."""

    def extensions(self, extensions: ExtensionsType) -> Self:
        """Arbitrary per-request data storage. Useful for passing through data to middleware and response."""

    def streamed_read_buffer_limit(self, value: int) -> Self:
        """Max bytes buffered when reading streamed body."""

    @staticmethod
    def default_streamed_read_buffer_limit() -> int:
        """Default max bytes buffered when reading streamed body."""

class RequestBuilder(BaseRequestBuilder):
    """Request builder. Use `build()` or `build_streamed()` to create the request to send."""

    def build(self) -> ConsumedRequest:
        """Build request that full reads the response body on send()."""

    def build_streamed(self) -> StreamRequest:
        """Build request whose response body is streamed."""

    def body_stream(self, stream: Stream) -> Self:
        """Set streaming request body."""

    def with_middleware(self, middleware: Middleware) -> Self:
        """Register a middleware component (added after client level middlewares, executed in chain order)."""

class SyncRequestBuilder(BaseRequestBuilder):
    """Synchronous request builder. Use `build()` or `build_streamed()` to create the request to send."""

    def build(self) -> SyncConsumedRequest:
        """Build request that full reads the response body on send()."""

    def build_streamed(self) -> SyncStreamRequest:
        """Build request whose response body is streamed."""

    def body_stream(self, stream: SyncStream) -> Self:
        """Set streaming request body."""

    def with_middleware(self, middleware: SyncMiddleware) -> Self:
        """Register a middleware component (added after client level middlewares, executed in chain order)."""
