"""HTTP client interfaces (async + sync) modeled after Rust reqwest.

`Client`/`SyncClient` is created via `ClientBuilder`/`SyncClientBuilder`.
Client should be reused for multiple requests.
"""

from datetime import timedelta
from typing import Any, Self

from pyreqwest.cookie import CookieStore
from pyreqwest.http import Url
from pyreqwest.middleware.types import Middleware, SyncMiddleware
from pyreqwest.proxy import ProxyBuilder
from pyreqwest.request import RequestBuilder, SyncRequestBuilder
from pyreqwest.types import HeadersType

from .types import JsonDumps, JsonLoads, SyncJsonLoads, TlsVersion

class BaseClient:
    """Common base for async and sync clients."""

class Client(BaseClient):
    """Asynchronous HTTP client. Inspired by reqwest's Client.

    Use as an async context manager for graceful shutdown. Can be also manually closed. Reuse for multiple requests.
    See also Rust reqwest [docs](https://docs.rs/reqwest/latest/reqwest/struct.Client.html) for more details.
    """

    async def __aenter__(self) -> Self:
        """Enter the async context manager (just returns self). @public"""

    async def __aexit__(self, *args: object, **kwargs: Any) -> None:
        """Close the client. @public"""

    def request(self, method: str, url: Url | str) -> RequestBuilder:
        """Start building a request with the method and url.

        Returns a request builder, which will allow setting headers and the request body before sending.
        """

    def get(self, url: Url | str) -> RequestBuilder:
        """Same as `request("GET", url)`."""

    def post(self, url: Url | str) -> RequestBuilder:
        """Same as `request("POST", url)`."""

    def put(self, url: Url | str) -> RequestBuilder:
        """Same as `request("PUT", url)`."""

    def patch(self, url: Url | str) -> RequestBuilder:
        """Same as `request("PATCH", url)`."""

    def delete(self, url: Url | str) -> RequestBuilder:
        """Same as `request("DELETE", url)`."""

    def head(self, url: Url | str) -> RequestBuilder:
        """Same as `request("HEAD", url)`."""

    async def close(self) -> None:
        """Close the client."""

class SyncClient(BaseClient):
    """Synchronous HTTP client. Inspired by reqwest's Client.

    Use as a context manager for graceful shutdown. Can be also manually closed. Reuse for multiple requests.
    See also Rust reqwest [docs](https://docs.rs/reqwest/latest/reqwest/struct.Client.html) for more details.
    """

    def __enter__(self) -> Self:
        """Enter the context manager (just returns self). @public"""

    def __exit__(self, *args: object, **kwargs: Any) -> None:
        """Exit the context manager and close resources. @public"""

    def request(self, method: str, url: Url | str) -> SyncRequestBuilder:
        """Start building a request with the method and url.

        Returns a request builder, which will allow setting headers and the request body before sending.
        """

    def get(self, url: Url | str) -> SyncRequestBuilder:
        """Same as `request("GET", url)`."""

    def post(self, url: Url | str) -> SyncRequestBuilder:
        """Same as `request("POST", url)`."""

    def put(self, url: Url | str) -> SyncRequestBuilder:
        """Same as `request("PUT", url)`."""

    def patch(self, url: Url | str) -> SyncRequestBuilder:
        """Same as `request("PATCH", url)`."""

    def delete(self, url: Url | str) -> SyncRequestBuilder:
        """Same as `request("DELETE", url)`."""

    def head(self, url: Url | str) -> SyncRequestBuilder:
        """Same as `request("HEAD", url)`."""

    def close(self) -> None:
        """Close the client."""

class BaseClientBuilder:
    def base_url(self, url: Url | str) -> Self:
        """Set a base URL automatically prepended to relative request URLs."""

    def runtime(self, runtime: "Runtime") -> Self:
        """Use a custom runtime (advanced scenarios). Usually not needed. By default, a global runtime is used."""

    def max_connections(self, max_connections: int | None) -> Self:
        """Maximum number of inflight requests. None means no limit. Default is None."""

    def error_for_status(self, enable: bool) -> Self:
        """Enable automatic HTTP error raising (4xx/5xx)."""

    def user_agent(self, value: str) -> Self:
        """Sets the User-Agent header to be used by this client (unless overridden).
        Default is `python-pyreqwest/1.0.0`.
        """

    def default_headers(self, headers: HeadersType) -> Self:
        """Sets the default headers for every request (unless overridden)."""

    def default_cookie_store(self, enable: bool) -> Self:
        """Enables default in-memory cookie store. Same as `cookie_store` in reqwest. Default is false."""

    def cookie_provider(self, provider: CookieStore) -> Self:
        """Set the cookie store for the client. Overrides `default_cookie_store`."""

    def gzip(self, enable: bool) -> Self:
        """Enable auto gzip decompression. Default is true."""

    def brotli(self, enable: bool) -> Self:
        """Enable auto brotli decompression. Default is true."""

    def zstd(self, enable: bool) -> Self:
        """Enable auto zstd decompression. Default is true."""

    def deflate(self, enable: bool) -> Self:
        """Enable auto deflate decompression. Default is true."""

    def max_redirects(self, max_redirects: int) -> Self:
        """Set maximum number of followed redirects. Default will follow redirects up to a maximum of 10."""

    def referer(self, enable: bool) -> Self:
        """Enable or disable automatic setting of the Referer header. Default is true."""

    def proxy(self, proxy: ProxyBuilder) -> Self:
        """Add a proxy based on ProxyBuilder to the list of proxies the Client will use."""

    def no_proxy(self) -> Self:
        """Clear all Proxies, so Client will use no proxy anymore."""

    def timeout(self, timeout: timedelta) -> Self:
        """Enables a total request timeout. Default is no timeout.

        The timeout is applied from when the request starts connecting until the response body has finished.
        Also considered a total deadline.
        """

    def read_timeout(self, timeout: timedelta) -> Self:
        """Enables a read timeout. Default is no timeout.

        The timeout applies to each read operation, and resets after a successful read. This is more appropriate for
        detecting stalled connections when the size isn't known beforehand.
        """

    def connect_timeout(self, timeout: timedelta) -> Self:
        """Set a timeout for only the connect phase of a Client. Default is None."""

    def pool_timeout(self, timeout: timedelta) -> Self:
        """Max wait time for an idle connection slot."""

    def pool_idle_timeout(self, timeout: timedelta | None) -> Self:
        """Set an optional timeout for idle sockets being kept-alive. Default is 90 seconds."""

    def pool_max_idle_per_host(self, max_idle: int) -> Self:
        """Sets the maximum idle connection per host allowed in the pool."""

    def http1_lower_case_headers(self) -> Self:
        """Send headers as lowercase instead of title case. Default is false.

        This differs from reqwest which uses lowercase by default.
        """

    def http1_allow_obsolete_multiline_headers_in_responses(self, value: bool) -> Self:
        """Set whether HTTP/1 connections will accept obsolete line folding for header values.

        Newline codepoints will be transformed to spaces when parsing.
        """

    def http1_ignore_invalid_headers_in_responses(self, value: bool) -> Self:
        """Sets whether invalid header lines should be silently ignored in HTTP/1 responses."""

    def http1_allow_spaces_after_header_name_in_responses(self, value: bool) -> Self:
        """Set whether HTTP/1 accepts spaces between header names and the colon that follow them in responses.

        Newline codepoints will be transformed to spaces when parsing.
        """

    def http1_only(self) -> Self:
        """Only use HTTP/1."""

    def http09_responses(self) -> Self:
        """Allow HTTP/0.9 responses (very old / uncommon)."""

    def http2_prior_knowledge(self) -> Self:
        """Only use HTTP/2."""

    def http2_initial_stream_window_size(self, value: int | None) -> Self:
        """Sets the SETTINGS_INITIAL_WINDOW_SIZE option for HTTP2 stream-level flow control. Default is 65K."""

    def http2_initial_connection_window_size(self, value: int | None) -> Self:
        """Sets the max connection-level flow control for HTTP2. Default is currently 65K."""

    def http2_adaptive_window(self, enabled: bool) -> Self:
        """Sets whether to use an adaptive flow control."""

    def http2_max_frame_size(self, value: int | None) -> Self:
        """Sets the maximum frame size to use for HTTP2. Default is currently 16K."""

    def http2_max_header_list_size(self, value: int) -> Self:
        """Sets the maximum size of received header frames for HTTP2. Default is currently 16KB."""

    def http2_keep_alive_interval(self, value: timedelta | None) -> Self:
        """Sets an interval for HTTP2 Ping frames should be sent to keep a connection alive. Default is disabled."""

    def http2_keep_alive_timeout(self, timeout: timedelta) -> Self:
        """Sets a timeout for receiving an acknowledgement of the keep-alive ping. Default is disabled."""

    def http2_keep_alive_while_idle(self, enabled: bool) -> Self:
        """Sets whether HTTP2 keep-alive should apply while the connection is idle. Default is false."""

    def tcp_nodelay(self, enabled: bool) -> Self:
        """Set TCP_NODELAY (disable Nagle). Default is true."""

    def local_address(self, addr: str | None) -> Self:
        """Bind to a local IP Address."""

    def interface(self, value: str) -> Self:
        """Bind connections only on the specified network interface."""

    def tcp_keepalive(self, duration: timedelta | None) -> Self:
        """Set SO_KEEPALIVE duration (overall TCP keepalive time)."""

    def tcp_keepalive_interval(self, interval: timedelta | None) -> Self:
        """Set SO_KEEPALIVE interval (TCP keepalive probe interval)."""

    def tcp_keepalive_retries(self, count: int | None) -> Self:
        """Set SO_KEEPALIVE retry count (number of failed keepalive probes before drop)."""

    def tcp_user_timeout(self, timeout: timedelta | None) -> Self:
        """Set TCP_USER_TIMEOUT (how long data may remain unacknowledged before the connection is force-closed)."""

    def add_root_certificate_der(self, cert: bytes) -> Self:
        """Trust additional DER root certificate."""

    def add_root_certificate_pem(self, cert: bytes) -> Self:
        """Trust additional PEM root certificate."""

    def add_crl_pem(self, cert: bytes) -> Self:
        """Add a certificate revocation list from PEM data."""

    def tls_built_in_root_certs(self, enable: bool) -> Self:
        """Toggle built-in root cert usage. Defaults to true - built-in system certs will be used."""

    def identity_pem(self, buf: bytes) -> Self:
        """Sets the identity to be used for client certificate authentication."""

    def danger_accept_invalid_hostnames(self, enable: bool) -> Self:
        """Disable hostname verification (INSECURE). Defaults to false."""

    def danger_accept_invalid_certs(self, enable: bool) -> Self:
        """Disable certificate validation (INSECURE). Defaults to false."""

    def tls_sni(self, enable: bool) -> Self:
        """Enable / disable TLS server name indication. Defaults to true."""

    def min_tls_version(self, value: TlsVersion) -> Self:
        """Set minimum accepted TLS version."""

    def max_tls_version(self, value: TlsVersion) -> Self:
        """Set maximum accepted TLS version."""

    def https_only(self, enable: bool) -> Self:
        """Refuse plain HTTP (HTTPS required). Defaults to false."""

    def resolve(self, domain: str, ip: str, port: int) -> Self:
        """Add static DNS resolution mapping (domain -> ip:port)."""

class ClientBuilder(BaseClientBuilder):
    """Fluent builder for configuring an async `Client`.

    After configuring options, call `build()` to obtain a `Client`.
    See also Rust reqwest [docs](https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html) for more details.
    """

    def __init__(self) -> None:
        """Create a new builder with default settings."""

    def build(self) -> Client:
        """Finalize and construct the async client.

        Fails if a TLS backend cannot be initialized, or the resolver cannot load the system configuration.
        """

    def with_middleware(self, middleware: Middleware) -> Self:
        """Register a middleware component (executed in chain order)."""

    def json_handler(self, *, loads: JsonLoads | None = ..., dumps: JsonDumps | None = ...) -> Self:
        """Override JSON loads / dumps callables for this client."""

class SyncClientBuilder(BaseClientBuilder):
    """Fluent builder for configuring a synchronous `SyncClient` (blocking style).

    After configuring options, call `build()` to obtain a `Client`.
    See also Rust reqwest [docs](https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html) for more details.
    """

    def __init__(self) -> None:
        """Create a new builder with default settings."""

    def build(self) -> SyncClient:
        """Finalize and construct the sync client.

        Fails if a TLS backend cannot be initialized, or the resolver cannot load the system configuration.
        """

    def with_middleware(self, middleware: SyncMiddleware) -> Self:
        """Register a middleware component (executed in chain order)."""

    def json_handler(self, *, loads: SyncJsonLoads | None = ..., dumps: JsonDumps | None = ...) -> Self:
        """Override JSON loads / dumps callables for this sync client."""

class Runtime:
    """Tokio runtime instance. Usually not needed, as library global runtime is used by default."""

    def __init__(self) -> None:
        """Create a tokio runtime instance. This is an advanced feature."""

    async def close(self) -> None:
        """Shutdown runtime resources. Clients using this runtime won't work anymore after closing."""
