from typing import Any, Self

from pyreqwest.bytes import Bytes
from pyreqwest.http import HeaderMap, Mime
from pyreqwest.types import ExtensionsType, HeadersType, Stream

class BaseResponse:
    @property
    def status(self) -> int:
        """HTTP status code (e.g. 200, 404)."""

    @status.setter
    def status(self, value: int) -> None:
        """Set HTTP status code."""

    @property
    def headers(self) -> HeaderMap:
        """Get the headers. This is not a copy. Modifying it modifies the response.
        You can also use `get_header` or `get_header_all` to access headers.
        """

    @headers.setter
    def headers(self, headers: HeadersType) -> None:
        """Replace headers. Given value is copied."""

    @property
    def extensions(self) -> dict[str, Any]:
        """Arbitrary per-request data storage. This is the data that was passed via request and middlewares.
        This is not a copy. Modifying it modifies the response.
        """

    @extensions.setter
    def extensions(self, value: ExtensionsType) -> None:
        """Replace extensions. Given value is shallow copied."""

    @property
    def version(self) -> str:
        """Used HTTP version (e.g. 'HTTP/1.1', 'HTTP/2.0')."""

    @version.setter
    def version(self, value: str) -> None:
        """Set HTTP version."""

    def error_for_status(self) -> None:
        """Raise StatusError for 4xx/5xx."""

    def get_header(self, key: str) -> str | None:
        """Return first matching header value else None (case-insensitive)."""

    def get_header_all(self, key: str) -> list[str]:
        """Return all values for header name (case-insensitive). Empty if absent."""

    def content_type_mime(self) -> Mime | None:
        """Parsed Content-Type header as Mime or None if absent."""

class Response(BaseResponse):
    """Asynchronous response with optionally streamed body."""

    async def bytes(self) -> Bytes:
        """Return entire body as bytes (cached after first read)."""

    async def json(self) -> Any:
        """Decode body as JSON (underlying bytes cached after first read). Uses serde for decoding.
        User can provide custom deserializer via `ClientBuilder.json_handler`.
        """

    async def text(self) -> str:
        """Decode body to text (underlying bytes cached after first read). Uses charset from Content-Type."""

    @property
    def body_reader(self) -> "ResponseBodyReader":
        """Access streaming reader. Using bytes(), json() or text() is not allowed after reading body partially."""

class SyncResponse(BaseResponse):
    """Synchronous response variant."""

    def bytes(self) -> Bytes:
        """Return entire body as bytes (cached after first read)."""

    def json(self) -> Any:
        """Decode body as JSON (underlying bytes cached after first read). Uses serde for decoding.
        User can provide custom deserializer via `SyncClientBuilder.json_handler`.
        """

    def text(self) -> str:
        """Decode body to text (underlying bytes cached after first read). Uses charset from Content-Type."""

    @property
    def body_reader(self) -> "SyncResponseBodyReader":
        """Access streaming reader. Using bytes(), json() or text() is not allowed after reading body partially."""

class ResponseBuilder:
    """Programmatic response construction (for testing, middlewares, manual responses)."""

    def __init__(self) -> None:
        """Create empty response builder (defaults: 200, HTTP/1.1, empty headers/body)."""

    async def build(self) -> Response:
        """Build asynchronous response."""

    def build_sync(self) -> SyncResponse:
        """Build synchronous response (disallows async streams)."""

    def status(self, value: int) -> Self:
        """Set status code."""

    def version(self, value: str) -> Self:
        """Set HTTP version string."""

    def header(self, name: str, value: str) -> Self:
        """Append single header value (multiple allowed)."""

    def headers(self, headers: HeadersType) -> Self:
        """Merge multiple headers (mapping or sequence)."""

    def extensions(self, extensions: ExtensionsType) -> Self:
        """Set extensions."""

    def body_bytes(self, body: bytes | bytearray | memoryview) -> Self:
        """Set fixed byte body (zero-copied where possible)."""

    def body_text(self, body: str) -> Self:
        """Set text body (UTF-8 encoded)."""

    def body_json(self, body: Any) -> Self:
        """Serialize body to JSON (sets Content-Type). Uses serde for serialization."""

    def body_stream(self, stream: Stream) -> Self:
        """Set streaming body. `build_sync` can not be mixed with async streams."""

    def copy(self) -> Self:
        """Copy builder."""
    def __copy__(self) -> Self: ...

class ResponseBodyReader:
    """Streaming body reader."""

    async def bytes(self) -> Bytes:
        """Read remaining stream fully and return bytes (caches)."""

    async def read(self, amount: int = ...) -> Bytes | None:
        """Read up to amount bytes (or default chunk size) from stream. None when stream is exhausted."""

    async def read_chunk(self) -> Bytes | None:
        """Return next raw chunk. Sizes are arbitrary and depend on OS. None when stream is exhausted."""

class SyncResponseBodyReader:
    """Streaming body reader."""

    def bytes(self) -> Bytes:
        """Read remaining stream fully and return bytes (caches)."""

    def read(self, amount: int = ...) -> Bytes | None:
        """Read up to amount bytes (or default chunk size) from stream. None when stream is exhausted."""

    def read_chunk(self) -> Bytes | None:
        """Return next raw chunk. Sizes are arbitrary and depend on OS. None when stream is exhausted."""
