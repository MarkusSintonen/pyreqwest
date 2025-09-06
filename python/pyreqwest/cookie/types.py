"""Cookie types and interfaces."""

from typing import Protocol


class CookieProvider(Protocol):
    """Cookie provider that allows custom cookie handling."""

    def set_cookies(self, cookie_headers: list[str], url: str) -> None:
        """Set cookies for a given URL.

        This method is called when the HTTP client receives a Set-Cookie header
        from a server response. Users should override this method to implement
        custom cookie storage logic.

        Args:
            cookie_headers: List of Set-Cookie header values received from url
            url: The URL that sent the Set-Cookie headers
        """

    def cookies(self, url: str) -> str | None:
        """Get cookies for a given URL.

        This method is called when the HTTP client is about to make a request
        and needs to determine which cookies to send. Users should override
        this method to implement custom cookie retrieval logic.

        Args:
            url: The URL for which cookies are requested

        Returns:
            A string containing the Cookie header value, or None if no cookies
        """
