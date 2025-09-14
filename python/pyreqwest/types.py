"""Common types and interfaces used in the library."""

from collections.abc import AsyncIterable, Iterable, Mapping, Sequence
from typing import Any

HeadersType = Mapping[str, str] | Sequence[tuple[str, str]]
QueryParams = Mapping[str, Any] | Sequence[tuple[str, Any]]
FormParams = Mapping[str, Any] | Sequence[tuple[str, Any]]
ExtensionsType = Mapping[str, Any] | Sequence[tuple[str, Any]]

SyncStream = Iterable[bytes] | Iterable[bytearray] | Iterable[memoryview]
Stream = AsyncIterable[bytes] | AsyncIterable[bytearray] | AsyncIterable[memoryview] | SyncStream
