"""HTTP cookie types backed by Rust's cookie and cookie_store crates."""

from collections.abc import Sequence
from datetime import datetime, timedelta
from typing import Any, Literal, Self, TypeAlias, overload

from pyreqwest.http import Url

SameSite: TypeAlias = Literal["Strict", "Lax", "None"]

class Cookie:
    """An immutable HTTP cookie. Lightweight Python wrapper around the internal Rust cookie::Cookie type.
    Use `with_*` methods to create modified copies of a Cookie.

    See also Rust [docs](https://docs.rs/cookie/latest/cookie/struct.Cookie.html) for more details.
    """

    def __init__(self, name: str, value: str) -> None:
        """Create a cookie with the given name and value (no attributes)."""

    @staticmethod
    def parse(cookie: str) -> "Cookie":
        """Parses a Cookie from the given HTTP cookie header value string."""

    @staticmethod
    def parse_encoded(cookie: str) -> "Cookie":
        """Like parse, but does percent-decoding of keys and values."""

    @staticmethod
    def split_parse(cookie: str) -> list["Cookie"]:
        """Parses the HTTP Cookie header, a series of cookie names and value separated by `;`."""

    @staticmethod
    def split_parse_encoded(cookie: str) -> list["Cookie"]:
        """Like split_parse, but does percent-decoding of keys and values."""

    @property
    def name(self) -> str:
        """Cookie name."""

    @property
    def value(self) -> str:
        """Raw cookie value as set (may contain surrounding whitespace)."""

    @property
    def value_trimmed(self) -> str:
        """Value with surrounding whitespace trimmed."""

    @property
    def http_only(self) -> bool:
        """Whether the HttpOnly attribute is set."""

    @property
    def secure(self) -> bool:
        """Whether the Secure attribute is set."""

    @property
    def same_site(self) -> SameSite | None:
        """SameSite attribute, or None if unspecified."""

    @property
    def partitioned(self) -> bool:
        """Whether the Partitioned attribute is set."""

    @property
    def max_age(self) -> timedelta | None:
        """Max-Age attribute duration, or None if not present."""

    @property
    def path(self) -> str | None:
        """Path attribute that scopes the cookie, or None if not present."""

    @property
    def domain(self) -> str | None:
        """Domain attribute that scopes the cookie, or None if not present."""

    @property
    def expires_datetime(self) -> datetime | None:
        """Absolute expiration time (Expires), or None if not present."""

    def encode(self) -> str:
        """Returns cookie string with percent-encoding applied."""

    def stripped(self) -> str:
        """Return just the 'name=value' pair."""

    def with_name(self, name: str) -> Self:
        """Set name, returning a new Cookie."""

    def with_value(self, value: str) -> Self:
        """Set value, returning a new Cookie."""

    def with_http_only(self, http_only: bool) -> Self:
        """Set HttpOnly attribute, returning a new Cookie."""

    def with_secure(self, secure: bool) -> Self:
        """Set Secure attribute, returning a new Cookie."""

    def with_same_site(self, same_site: SameSite | None) -> Self:
        """Set SameSite attribute, returning a new Cookie."""

    def with_partitioned(self, partitioned: bool) -> Self:
        """Set Partitioned attribute, returning a new Cookie."""

    def with_max_age(self, max_age: timedelta | None) -> Self:
        """Set Max-Age attribute, returning a new Cookie."""

    def with_path(self, path: str | None) -> Self:
        """Set Path attribute, returning a new Cookie."""

    def with_domain(self, domain: str | None) -> Self:
        """Set Domain attribute, returning a new Cookie."""

    def with_expires_datetime(self, expires: datetime | None) -> Self:
        """Set Expires attribute, returning a new Cookie."""

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

class CookieStore:
    """Thread-safe in-memory cookie store (domain/path aware). Mirrors the behavior of Rust's cookie_store.

    See also Rust [docs](https://docs.rs/cookie_store/latest/cookie_store/struct.CookieStore.html) for more details.
    """

    def __init__(self) -> None:
        """Create an empty cookie store."""

    def contains(self, domain: str, path: str, name: str) -> bool:
        """Returns true if the CookieStore contains an unexpired Cookie corresponding to the specified domain, path,
        and name.
        """

    def contains_any(self, domain: str, path: str, name: str) -> bool:
        """Returns true if the CookieStore contains any (even an expired) Cookie corresponding to the specified
        domain, path, and name.
        """

    def get(self, domain: str, path: str, name: str) -> Cookie | None:
        """Returns a reference to the unexpired Cookie corresponding to the specified domain, path, and name."""

    def get_any(self, domain: str, path: str, name: str) -> Cookie | None:
        """Returns a reference to the (possibly expired) Cookie corresponding to the specified domain, path, and
        name.
        """

    def remove(self, domain: str, path: str, name: str) -> Cookie | None:
        """Removes a Cookie from the store, returning the Cookie if it was in the store."""

    def matches(self, url: Url | str) -> list[Cookie]:
        """Returns a collection of references to unexpired cookies that path- and domain-match request_url, as well as
        having HttpOnly and Secure attributes compatible with the request_url.
        """

    def insert(self, cookie: Cookie | str, request_url: Url | str) -> None:
        """Insert a cookie as if set by a response for request_url."""

    def clear(self) -> None:
        """Remove all cookies from the store."""

    def get_all_unexpired(self) -> list[Cookie]:
        """Return all unexpired cookies currently stored."""

    def get_all_any(self) -> list[Cookie]:
        """Return all cookies in the store, including expired ones."""
