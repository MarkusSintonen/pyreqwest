"""HTTP cookie types backed by Rust's cookie and cookie_store crates.

Most builder and cookie concepts mirror Rust crates design while providing a Pythonic surface.
See docs:
- cookie: https://docs.rs/cookie/latest/cookie/struct.Cookie.html
- cookie_store: https://docs.rs/cookie_store/latest/cookie_store/struct.CookieStore.html
"""

from collections.abc import Sequence
from datetime import datetime, timedelta
from typing import Literal, Self, TypeAlias, overload

from pyreqwest.http import Url

SameSite: TypeAlias = Literal["Strict", "Lax", "None"]

class Cookie(Sequence[str]):
    """An immutable HTTP cookie (name, value, and optional attributes). Mirrors the behavior of Rust's cookie create."""

    def __init__(self, name: str, value: str) -> None:
        """Create a cookie with the given name and value (no attributes).

        Use CookieBuilder or parsing methods to create cookies with attributes.
        """

    @staticmethod
    def parse(cookie: str) -> Cookie:
        """Parses a Cookie from the given HTTP cookie header value string."""

    @staticmethod
    def parse_encoded(cookie: str) -> Cookie:
        """Like parse, but does percent-decoding of keys and values."""

    @staticmethod
    def split_parse(cookie: str) -> list[Cookie]:
        """Parses the HTTP Cookie header, a series of cookie names and value separated by `;`."""

    @staticmethod
    def split_parse_encoded(cookie: str) -> list[Cookie]:
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

    def __copy__(self) -> Cookie:
        """Copy the cookie."""

    def __hash__(self) -> int:
        """Return a hash based on the cookie's string representation."""

    def __eq__(self, other: object) -> bool:
        """Return True if cookies are equal (by underlying attributes)."""

    def __ne__(self, other: object) -> bool:
        """Return True if cookies are not equal."""

    def __len__(self) -> int:
        """Length of the string representation of the cookie."""

    @overload
    def __getitem__(self, index: int) -> str: ...
    @overload
    def __getitem__(self, index: slice) -> str: ...

class CookieBuilder:
    """Fluent builder for Cookie instances. The builder is single-use calling build consumes it.

    Mirrors the behavior of Rust's cookie crate.
    """

    def __init__(self, name: str, value: str) -> None:
        """Start a builder for a cookie with name and value."""

    @staticmethod
    def from_cookie(cookie: Cookie | str) -> CookieBuilder:
        """Start a builder pre-populated from an existing Cookie."""

    def build(self) -> Cookie:
        """Build and return the Cookie. Consumes the builder."""

    def expires(self, expires: datetime | None) -> Self:
        """Set the Expires attribute (absolute time) or clear it with None."""

    def max_age(self, max_age: timedelta) -> Self:
        """Set the Max-Age attribute (relative lifetime)."""

    def domain(self, domain: str) -> Self:
        """Set the Domain attribute."""

    def path(self, path: str) -> Self:
        """Set the Path attribute."""

    def secure(self, secure: bool) -> Self:
        """Enable or disable the Secure attribute."""

    def http_only(self, http_only: bool) -> Self:
        """Enable or disable the HttpOnly attribute."""

    def same_site(self, same_site: SameSite) -> Self:
        """Set the SameSite attribute."""

    def partitioned(self, partitioned: bool) -> Self:
        """Enable or disable the Partitioned attribute."""

    def permanent(self) -> Self:
        """Set a long-lived expiration (far future) per cookie crate semantics."""

    def removal(self) -> Self:
        """Configure as a removal cookie (empty value, expired in the past)."""

class CookieStore:
    """Thread-safe in-memory cookie store (domain/path aware). Mirrors the behavior of Rust's cookie_store."""
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
