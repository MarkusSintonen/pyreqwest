from pathlib import Path
from typing import Self

from pyreqwest.http import Mime
from pyreqwest.types import HeadersType, Stream

class FormBuilder:
    """Build multipart/form-data. Chain calls (text, file, part, encoding) then pass to RequestBuilder.multipart()."""
    def __init__(self) -> None:
        """Creates form builder without any content."""

    @property
    def boundary(self) -> str:
        """Get the boundary that this form will use."""

    def text(self, name: str, value: str) -> Self:
        """Add a data field with supplied name and value."""

    async def file(self, name: str, path: Path) -> Self:
        """Makes a file parameter."""

    def sync_file(self, name: str, path: Path) -> Self:
        """Makes a file parameter. File read is blocking."""

    def part(self, name: str, part: "PartBuilder") -> Self:
        """Adds a customized part from PartBuilder."""

    def percent_encode_path_segment(self) -> Self:
        """Configure this Form to percent-encode using the path-segment rules. This is the default."""

    def percent_encode_attr_chars(self) -> Self:
        """Configure this Form to percent-encode using the attr-char rules."""

    def percent_encode_noop(self) -> Self:
        """Configure this Form to skip percent-encoding."""

class PartBuilder:
    """Build an individual multipart part. Create with from_* then optionally set mime, filename, headers.
    Add via FormBuilder.part().
    """

    @staticmethod
    def from_text(value: str) -> "PartBuilder":
        """Makes a text parameter."""

    @staticmethod
    def from_bytes(value: bytes) -> "PartBuilder":
        """Makes a new parameter from arbitrary bytes."""

    @staticmethod
    def from_stream(stream: Stream) -> "PartBuilder":
        """Makes a new parameter from an arbitrary stream."""

    @staticmethod
    def from_stream_with_length(stream: Stream, length: int) -> "PartBuilder":
        """Makes a new parameter from an arbitrary stream with a known length. This is particularly useful when adding
        something like file contents as a stream, where you can know the content length beforehand.
        """

    @staticmethod
    async def from_file(path: Path) -> "PartBuilder":
        """Makes a file parameter."""

    @staticmethod
    def from_sync_file(path: Path) -> "PartBuilder":
        """Makes a file parameter. File read is blocking."""

    def mime(self, mime: Mime | str) -> Self:
        """Set the mime of this part."""

    def file_name(self, filename: str) -> Self:
        """Set the filename."""

    def headers(self, headers: HeadersType) -> Self:
        """Sets custom headers for the part."""
