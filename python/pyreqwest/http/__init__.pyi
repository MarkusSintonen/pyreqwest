from collections.abc import ItemsView, Iterator, KeysView, MutableMapping, Sequence, ValuesView
from typing import Any, Self, TypeVar, overload

from pyreqwest.types import HeadersType, QueryParams

_T = TypeVar("_T")

class Url:
    """Immutable parsed URL. Python wrapper around the internal Rust url::Url type.

    See also Rust [docs](https://docs.rs/url/latest/url/struct.Url.html) for more details.
    """

    def __init__(self, url: str) -> None:
        """Parse an absolute URL from a string."""

    @staticmethod
    def parse(url: str) -> "Url":
        """Parse an absolute URL from a string. Same as Url(url)."""

    @staticmethod
    def parse_with_params(url: str, params: QueryParams) -> "Url":
        """Parse an absolute URL from a string and add params to its query string. Existing params are not removed."""

    def join(self, join_input: str) -> Self:
        """Parse a string as a URL, with this URL as the base URL. The inverse of this is make_relative.

        Notes:
        - A trailing slash is significant. Without it, the last path component is considered to be a “file” name to be
        removed to get at the “directory” that is used as the base.
        - A scheme relative special URL   as input replaces everything in the base URL after the scheme.
        - An absolute URL (with a scheme) as input replaces the whole base URL (even the scheme).
        """

    def make_relative(self, base: Self | str) -> str | None:
        """Creates a relative URL if possible, with this URL as the base URL. This is the inverse of join."""

    @property
    def origin_ascii(self) -> str:
        """Return the origin of this URL ASCII-serialized."""

    @property
    def origin_unicode(self) -> str:
        """Return the origin of this URL Unicode-serialized."""

    @property
    def scheme(self) -> str:
        """Return the scheme of this URL, lower-cased, as an ASCII string without the ':' delimiter."""

    @property
    def is_special(self) -> bool:
        """Whether the scheme is a WHATWG "special" scheme (http, https, ws, wss, ftp, file)."""

    @property
    def has_authority(self) -> bool:
        """Return whether the URL has an 'authority', which can contain a username, password, host, and port number.

        URLs that do not are either path-only like unix:/run/foo.socket or cannot-be-a-base like data:text/plain,Stuff.
        See also the `authority` method.
        """

    @property
    def authority(self) -> str:
        """Return the authority of this URL as an ASCII string.

        Non-ASCII domains are punycode-encoded per IDNA if this is the host of a special URL, or percent encoded for
        non-special URLs. IPv6 addresses are given between [ and ] brackets. Ports are omitted if they match the well
        known port of a special URL. Username and password are percent-encoded.
        See also the `has_authority` method.
        """

    @property
    def cannot_be_a_base(self) -> bool:
        """Return whether this URL is a cannot-be-a-base URL, meaning that parsing a relative URL string with this URL
        as the base will return an error.

        This is the case if the scheme and : delimiter are not followed by a / slash, as is typically the case of data:
        and mailto: URLs.
        """

    @property
    def username(self) -> str:
        """Return the username for this URL (typically the empty string) as a percent-encoded ASCII string."""

    @property
    def password(self) -> str | None:
        """Return the password for this URL, if any, as a percent-encoded ASCII string."""

    @property
    def has_host(self) -> bool:
        """Equivalent to bool(url.host_str)."""

    @property
    def host_str(self) -> str | None:
        """Return the string representation of the host (domain or IP address) for this URL, if any.

        Non-ASCII domains are punycode-encoded per IDNA if this is the host of a special URL, or percent encoded for
        non-special URLs. IPv6 addresses are given between [ and ] brackets.
        Cannot-be-a-base URLs (typical of data: and mailto:) and some file: URLs don't have a host.
        """

    @property
    def domain(self) -> str | None:
        """If this URL has a host and it is a domain name (not an IP address), return it. Non-ASCII domains are
        punycode-encoded per IDNA if this is the host of a special URL, or percent encoded for non-special URLs.
        """

    @property
    def port(self) -> int | None:
        """Return the port number for this URL, if any."""

    @property
    def port_or_known_default(self) -> int | None:
        """Return the port number for this URL, or the default port number if it is known.

        This method only knows the default port number of the http, https, ws, wss and ftp schemes.
        """

    @property
    def path(self) -> str:
        """Return the path for this URL, as a percent-encoded ASCII string. For cannot-be-a-base URLs, this is an
        arbitrary string that doesn't start with '/'. For other URLs, this starts with a '/' slash and continues with
        slash-separated path segments.
        """

    @property
    def path_segments(self) -> list[str] | None:
        """Unless this URL is cannot-be-a-base, return a list of '/' slash-separated path segments, each as a
        percent-encoded ASCII string. Return None for cannot-be-a-base URLs. When list is returned, it always contains
        at least one string (which may be empty).
        """

    @property
    def query_string(self) -> str | None:
        """Return this URL's query string, if any, as a percent-encoded ASCII string."""

    @property
    def query_pairs(self) -> list[tuple[str, str]]:
        """Parse the URL's query string, if any, as urlencoded and return list of (key, value) pairs."""

    @property
    def query_dict_multi_value(self) -> dict[str, str | list[str]]:
        """Parse the URL's query string, if any, as urlencoded and return dict where repeated keys become a list
        preserving order.
        """

    @property
    def fragment(self) -> str | None:
        """Return this URL's fragment identifier, if any. A fragment is the part of the URL after the # symbol."""

    def with_query(self, query: QueryParams | None) -> Self:
        """Replace the entire query with provided params (None removes query)."""

    def extend_query(self, query: QueryParams) -> Self:
        """Append additional key/value pairs to existing query keeping original order."""

    def with_query_string(self, query: str | None) -> Self:
        """Replace query using a preformatted string (no leading '?'). None removes it."""

    def with_path(self, path: str) -> Self:
        """Return a copy with a new path. Accepts with/without leading '/'. Empty path means '/'."""

    def with_path_segments(self, segments: list[str]) -> Self:
        """Append each segment from the given list at the end of this URL's path.
        Each segment is percent-encoded, except that % and / characters are also encoded (to %25 and %2F).
        """

    def with_port(self, port: int | None) -> Self:
        """Change this URL's port number. None removes explicit port."""

    def with_host(self, host: str | None) -> Self:
        """Change this URL's host.
        Removing the host (calling this with None) will also remove any username, password, and port number.
        """

    def with_ip_host(self, addr: str) -> Self:
        """Change this URL's host to the given IP address."""

    def with_username(self, username: str) -> Self:
        """Change this URL's username."""

    def with_password(self, password: str | None) -> Self:
        """Change this URL's password."""

    def with_scheme(self, scheme: str) -> Self:
        """Change this URL's scheme."""

    def with_fragment(self, fragment: str | None) -> Self:
        """Change this URL's fragment identifier."""

    def __truediv__(self, join_input: str) -> Self:
        """Path join shorthand: url / 'segment' == url.join('segment')."""

    def __contains__(self, item: Any) -> bool: ...
    def __copy__(self) -> Self: ...
    def __hash__(self) -> int: ...
    @overload
    def __getitem__(self, index: int) -> str: ...
    @overload
    def __getitem__(self, index: slice) -> Sequence[str]: ...
    def __len__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...
    def __lt__(self, other: object) -> bool: ...
    def __le__(self, other: object) -> bool: ...

class Mime:
    """Parsed media (MIME) type. Lightweight Python wrapper around the internal Rust mime::Mime type.

    See also Rust [docs](https://docs.rs/mime/latest/mime/struct.Mime.html) for more details.
    """

    @staticmethod
    def parse(mime: str) -> "Mime":
        """Parse a MIME string into a `Mime`."""

    @property
    def type_(self) -> str:
        """Lowercased top-level type (before '/')."""

    @property
    def subtype(self) -> str:
        """Lowercased subtype (after '/'), excluding any +suffix."""

    @property
    def suffix(self) -> str | None:
        """Structured syntax suffix (text after '+') or None."""

    @property
    def parameters(self) -> list[tuple[str, str]]:
        """List of (name, value) parameter pairs in original order. Names are lowercase; duplicates kept."""

    @property
    def essence_str(self) -> str:
        """type/subtype(+suffix) without parameters (RFC 6838 essence)."""

    def get_param(self, name: str) -> str | None:
        """Return first parameter value whose name (case-insensitive) matches, else None."""

    def __contains__(self, item: Any) -> bool: ...
    def __copy__(self) -> Self: ...
    def __hash__(self) -> int: ...
    @overload
    def __getitem__(self, index: int) -> str: ...
    @overload
    def __getitem__(self, index: slice) -> Sequence[str]: ...
    def __len__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...
    def __lt__(self, other: object) -> bool: ...
    def __le__(self, other: object) -> bool: ...

class HeaderMapItemsView(ItemsView[str, str]):
    """Live view of (key,value) pairs in insertion order."""
    def __eq__(self, other: object) -> bool: ...

class HeaderMapKeysView(KeysView[str]):
    """Live view of header keys (one per stored value)."""
    def __eq__(self, other: object) -> bool: ...

class HeaderMapValuesView(ValuesView[str]):
    """Live view of values in insertion order."""
    def __eq__(self, other: object) -> bool: ...

class HeaderMap(MutableMapping[str, str]):
    """Case-insensitive multi-value HTTP header map."""

    def __init__(self, other: HeadersType | None = None) -> None:
        """Init from: list/tuple of (key,value), dict (single values), HeaderMap, CIMultiDict."""

    def __len__(self) -> int:
        """Number of items in the map."""

    def __iter__(self) -> Iterator[str]:
        """Iterate over header keys (one entry per stored value, so duplicates repeat)."""

    def __getitem__(self, key: str, /) -> str:
        """First value for key."""

    def __setitem__(self, key: str, value: str, /) -> None:
        """Set single value for key; replaces all existing values for that key."""

    def __delitem__(self, key: str, /) -> None:
        """Delete all values for key."""

    def __eq__(self, other: object) -> bool:
        """Equality: per-key value sequence must match; inter-key ordering ignored.
        Supports mappings and key/value pair iterables.
        """

    def items(self) -> HeaderMapItemsView:
        """Live view of (key,value) pairs in insertion order."""

    def keys(self) -> HeaderMapKeysView:
        """Live view of header keys (one per stored value)."""

    def values(self) -> HeaderMapValuesView:
        """Live view of values in insertion order."""

    def keys_len(self) -> int:
        """Count of distinct header keys."""

    def getall(self, key: str) -> list[str]:
        """All values for key. Empty if key does not exist."""

    def insert(self, key: str, value: str, *, is_sensitive: bool = False) -> list[str]:
        """Replace all existing values for key with value. Returns list of removed values (may be empty)."""

    def append(self, key: str, value: str, *, is_sensitive: bool = False) -> bool:
        """Add value for a key. If the map did have this key present, the new value is pushed to the end of the list of
        values currently associated with the key. Return True if key already existed, else False.
        """

    def extend(self, other: HeadersType) -> None:
        """Append pairs from mapping / iterable."""

    @overload
    def popall(self, key: str) -> list[str]:
        """Remove all values for key and return list. With default returns it instead of raising KeyError."""
    @overload
    def popall(self, key: str, /, default: _T) -> list[str] | _T:
        """Remove all values for key and return list. With default returns it instead of raising KeyError."""

    def dict_multi_value(self) -> dict[str, str | list[str]]:
        """Dict: single-value headers -> str; multi-value -> list[str]."""

    def copy(self) -> Self:
        """Copy the map."""

    def __copy__(self) -> Self: ...
    @overload
    def pop(self, key: str) -> str:
        """Remove & return FIRST value for key. With default returns default instead of KeyError."""
    @overload
    def pop(self, key: str, default: _T = ...) -> str | _T:
        """Remove & return FIRST value for key. With default returns default instead of KeyError."""

    def popitem(self) -> tuple[str, str]:
        """Remove & return an arbitrary (key, FIRST value) respecting value ordering; KeyError if empty."""

    def clear(self) -> None:
        """Empty the map."""
