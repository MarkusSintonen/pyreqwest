"""Common types and interfaces used in the library."""

from collections.abc import AsyncIterable, Mapping, Sequence
from typing import Any

from pyreqwest.http import Url

UrlType = Url | str
HeadersType = Mapping[str, str] | Sequence[tuple[str, str]]
QueryParams = Mapping[str, Any] | Sequence[tuple[str, Any]]
FormParams = Mapping[str, Any] | Sequence[tuple[str, Any]]
ExtensionsType = Mapping[str, Any] | Sequence[tuple[str, Any]]
Stream = AsyncIterable[bytes] | AsyncIterable[bytearray] | AsyncIterable[memoryview]
