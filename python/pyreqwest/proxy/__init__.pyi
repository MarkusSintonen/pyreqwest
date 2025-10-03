from collections.abc import Callable
from typing import Self

from pyreqwest.http import Url
from pyreqwest.types import HeadersType

class ProxyBuilder:
    """Configuration of a proxy that a Client should pass requests to.

    The resulting instance is passed to `ClientBuilder.proxy(...)`.
    Based on reqwest's `Proxy` type.
    See also Rust [docs](https://docs.rs/reqwest/latest/reqwest/struct.Proxy.html) for more details.
    """

    @staticmethod
    def http(url: Url | str) -> "ProxyBuilder":
        """Proxy all HTTP traffic to the passed URL."""

    @staticmethod
    def https(url: Url | str) -> "ProxyBuilder":
        """Proxy all HTTPS traffic to the passed URL."""

    @staticmethod
    def all(url: Url | str) -> "ProxyBuilder":
        """Proxy all traffic to the passed URL.

        "All" refers to https and http URLs. Other schemes are not recognized.
        """

    @staticmethod
    def custom(fun: Callable[[Url], Url | str | None]) -> "ProxyBuilder":
        """Provide a custom function to determine what traffic to proxy to where.

        Any exception raised or an invalid/relative return value surfaces as a `RequestPanicError`.
        """

    def basic_auth(self, username: str, password: str) -> Self:
        """Set the Proxy-Authorization header using Basic auth."""

    def custom_http_auth(self, header_value: str) -> Self:
        """Set the Proxy-Authorization header to a specified value."""

    def headers(self, headers: HeadersType) -> Self:
        """Add custom headers."""

    def no_proxy(self, no_proxy_list: str | None) -> Self:
        """Adds a No Proxy exclusion list to this proxy."""
