"""Common types and interfaces used in the library."""

from collections.abc import AsyncIterable, Iterable, Mapping, Sequence
from typing import Any, TypeAlias

HeadersType: TypeAlias = Mapping[str, str] | Sequence[tuple[str, str]]
QueryParams: TypeAlias = Mapping[str, Any] | Sequence[tuple[str, Any]]
FormParams: TypeAlias = Mapping[str, Any] | Sequence[tuple[str, Any]]
ExtensionsType: TypeAlias = Mapping[str, Any] | Sequence[tuple[str, Any]]

SyncStream: TypeAlias = Iterable[bytes] | Iterable[bytearray] | Iterable[memoryview]
Stream: TypeAlias = AsyncIterable[bytes] | AsyncIterable[bytearray] | AsyncIterable[memoryview] | SyncStream
