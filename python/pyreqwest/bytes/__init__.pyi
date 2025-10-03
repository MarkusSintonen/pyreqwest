# Based on:
# https://github.com/developmentseed/obstore/blob/b87054eaac3a26ee65a1d4bc69a5e01fdab3d5d9/pyo3-bytes/README.md?plain=1#L58

import sys
from typing import Protocol, Self, overload

if sys.version_info >= (3, 12):
    from collections.abc import Buffer
else:
    class Buffer(Protocol):
        # Not actually a Protocol at runtime; see https://github.com/python/typeshed/issues/10224
        def __buffer__(self, flags: int, /) -> memoryview: ...

class Bytes(Buffer):
    """A `bytes`-like buffer.

    This implements the Python buffer protocol, allowing zero-copy access
    to underlying Rust memory.

    You can pass this to `memoryview` for a zero-copy view into the underlying
    data or to `bytes` to copy the underlying data into a Python `bytes`.
    You can also use `to_bytes` to copy the underlying data into a Python `bytes`.

    Many methods from the Python `bytes` class are implemented on this,
    """

    def __init__(self, buf: Buffer = b"") -> None:
        """Construct a new Bytes object.

        This will be a zero-copy view on the Python byte slice.
        """
    def __add__(self, other: Buffer) -> Self: ...
    def __buffer__(self, flags: int) -> memoryview: ...
    def __contains__(self, other: Buffer) -> bool: ...
    def __eq__(self, other: object) -> bool: ...
    @overload
    def __getitem__(self, key: int, /) -> int: ...
    @overload
    def __getitem__(self, key: slice, /) -> Self: ...
    def __getitem__(self, key: int | slice, /) -> int | Self: ...  # type: ignore[misc] # docstring in pyi file
    def __mul__(self, other: Buffer) -> int: ...
    def __len__(self) -> int: ...
    def removeprefix(self, prefix: Buffer, /) -> Self:
        """If the binary data starts with the prefix string, return `bytes[len(prefix):]`.
        Otherwise, return the original binary data.
        """
    def removesuffix(self, suffix: Buffer, /) -> Self:
        """If the binary data ends with the suffix string and that suffix is not empty,
        return `bytes[:-len(suffix)]`. Otherwise, return the original binary data.
        """
    def isalnum(self) -> bool:
        """Return `True` if all bytes in the sequence are alphabetical ASCII characters or
        ASCII decimal digits and the sequence is not empty, `False` otherwise.

        Alphabetic ASCII characters are those byte values in the sequence
        `b'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ'`. ASCII decimal digits
        are those byte values in the sequence `b'0123456789'`.
        """
    def isalpha(self) -> bool:
        """Return `True` if all bytes in the sequence are alphabetic ASCII characters and
        the sequence is not empty, `False` otherwise.

        Alphabetic ASCII characters are those byte values in the sequence
        `b'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ'`.
        """
    def isascii(self) -> bool:
        """Return `True` if the sequence is empty or all bytes in the sequence are ASCII,
        `False` otherwise.

        ASCII bytes are in the range `0-0x7F`.
        """
    def isdigit(self) -> bool:
        """Return `True` if all bytes in the sequence are ASCII decimal digits and the
        sequence is not empty, `False` otherwise.

        ASCII decimal digits are those byte values in the sequence `b'0123456789'`.
        """
    def islower(self) -> bool:
        """Return `True` if there is at least one lowercase ASCII character in the sequence
        and no uppercase ASCII characters, `False` otherwise.
        """
    def isspace(self) -> bool:
        r"""Return `True` if all bytes in the sequence are ASCII whitespace and the sequence
        is not empty, `False` otherwise.

        ASCII whitespace characters are those byte values
        in the sequence `b' \t\n\r\x0b\f'` (space, tab, newline, carriage return,
        vertical tab, form feed).
        """
    def isupper(self) -> bool:
        """Return `True` if there is at least one uppercase alphabetic ASCII character in
        the sequence and no lowercase ASCII characters, `False` otherwise.
        """

    def lower(self) -> Self:
        """Return a copy of the sequence with all the uppercase ASCII characters converted
        to their corresponding lowercase counterpart.
        """

    def upper(self) -> Self:
        """Return a copy of the sequence with all the lowercase ASCII characters converted
        to their corresponding uppercase counterpart.
        """

    def to_bytes(self) -> bytes:
        """Copy this buffer's contents into a Python `bytes` object."""
