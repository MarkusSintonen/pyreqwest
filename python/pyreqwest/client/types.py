"""Types for pyreqwest client module."""

from collections.abc import Awaitable, Callable
from typing import Any, Literal, Protocol

from pyreqwest.http import HeaderMap
from pyreqwest.response import ResponseBodyReader, SyncResponseBodyReader


class JsonDumpsContext(Protocol):
    """Data for json serializing."""

    @property
    def data(self) -> Any:
        """The data to be serialized to JSON."""


class JsonLoadsContext(Protocol):
    """Data for json deserializing."""

    @property
    def body_reader(self) -> ResponseBodyReader:
        """The body reader to read the JSON data from."""

    @property
    def headers(self) -> HeaderMap:
        """The response headers."""

    @property
    def extensions(self) -> dict[str, Any]:
        """The extensions associated with the request."""


class SyncJsonLoadsContext(Protocol):
    """Data for sync json deserializing."""

    @property
    def body_reader(self) -> SyncResponseBodyReader:
        """The body reader to read the JSON data from."""

    @property
    def headers(self) -> HeaderMap:
        """The response headers."""

    @property
    def extensions(self) -> dict[str, Any]:
        """The extensions associated with the request."""


JsonDumps = Callable[[JsonDumpsContext], bytes | bytearray | memoryview]
JsonLoads = Callable[[JsonLoadsContext], Awaitable[Any]]
SyncJsonLoads = Callable[[SyncJsonLoadsContext], Any]

TlsVersion = Literal["TLSv1.0", "TLSv1.1", "TLSv1.2", "TLSv1.3"]
