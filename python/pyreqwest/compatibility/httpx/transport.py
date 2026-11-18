"""Compatibility layer for httpx, which allows pyreqwest to replace httpcore transport for httpx."""

from collections.abc import AsyncIterator, Iterator
from contextlib import AsyncExitStack, ExitStack

import httpx

from pyreqwest.client import Client, ClientBuilder, SyncClient, SyncClientBuilder
from pyreqwest.exceptions import PyreqwestError
from pyreqwest.request import RequestBuilder, SyncRequestBuilder
from pyreqwest.response import Response, SyncResponse

from ._internal import build_httpx_response, map_exception, map_extensions


class HttpxTransport(httpx.AsyncBaseTransport):
    """httpx transport that uses pyreqwest for HTTP requests.

    Example usage:
    ```python
    import httpx
    from pyreqwest.compatibility.httpx import HttpxTransport

    async with httpx.AsyncClient(transport=HttpxTransport()) as httpx_client:
        print(await httpx_client.get("https://example.com"))
    ```
    """

    def __init__(self, client: Client | None = None, *, close_client: bool = True) -> None:
        """Initialize the HttpxTransport.
        :param client: An optional pyreqwest Client instance. If not provided, a default Client will be created.
            Note: httpx does not expose the used client features into its transport interface. Therefore, pyreqwest
            client should be configured for advanced use-cases. For example, cookie support must be enabled on the
            client explicitly if needed.
        :param close_client: Whether to close the provided Client when the transport is closed.
        """
        self._client: Client = client or ClientBuilder().build()
        self._close_client = (client is None) or close_client

    async def aclose(self) -> None:
        """Close the underlying pyreqwest Client if needed."""
        if self._close_client:
            await self._client.close()

    async def handle_async_request(self, request: httpx.Request) -> httpx.Response:
        """AsyncBaseTransport implementation to handle httpx request using pyreqwest."""
        req_builder = self._client.request(request.method, str(request.url)).headers(request.headers.multi_items())
        req_builder = self._map_body(req_builder, request)
        req_builder = map_extensions(req_builder, request)
        return await self._map_response(req_builder, request)

    def _map_body(self, builder: RequestBuilder, request: httpx.Request) -> RequestBuilder:
        try:
            return builder.body_bytes(request.content)
        except httpx.RequestNotRead:
            return builder.body_stream(request.stream)

    async def _map_response(self, builder: RequestBuilder, request: httpx.Request) -> httpx.Response:
        req_exit_stack = AsyncExitStack()
        try:
            response = await req_exit_stack.enter_async_context(builder.build_streamed())
            return build_httpx_response(response, response_stream=ResponseStream(response, req_exit_stack))
        except Exception as exc:
            await req_exit_stack.aclose()
            if isinstance(exc, PyreqwestError) and (mapped := map_exception(exc, request)):
                raise mapped from exc
            raise


class SyncHttpxTransport(httpx.BaseTransport):
    """httpx transport that uses pyreqwest for HTTP requests.

    Example usage:
    ```python
    import httpx
    from pyreqwest.compatibility.httpx import SyncHttpxTransport

    with httpx.Client(transport=SyncHttpxTransport()) as httpx_client:
        print(httpx_client.get("https://example.com"))
    ```
    """

    def __init__(self, client: SyncClient | None = None, *, close_client: bool = True) -> None:
        """Initialize the SyncHttpxTransport.
        :param client: An optional pyreqwest SyncClient instance. If not provided, a default SyncClient will be created.
            Note: httpx does not expose the used client features into its transport interface. Therefore, pyreqwest
            client should be configured for advanced use-cases. For example, cookie support must be enabled on the
            client explicitly if needed.
        :param close_client: Whether to close the provided SyncClient when the transport is closed.
        """
        self._client: SyncClient = client or SyncClientBuilder().build()
        self._close_client = (client is None) or close_client

    def close(self) -> None:
        """Close the underlying pyreqwest SyncClient if needed."""
        if self._close_client:
            self._client.close()

    def handle_request(self, request: httpx.Request) -> httpx.Response:
        """BaseTransport implementation to handle httpx request using pyreqwest."""
        req_builder = self._client.request(request.method, str(request.url)).headers(request.headers.multi_items())
        req_builder = self._map_body(req_builder, request)
        req_builder = map_extensions(req_builder, request)
        return self._map_response(req_builder, request)

    def _map_body(self, builder: SyncRequestBuilder, request: httpx.Request) -> SyncRequestBuilder:
        try:
            return builder.body_bytes(request.content)
        except httpx.RequestNotRead:
            if not isinstance(request.stream, httpx.SyncByteStream):
                err = "Cannot use async stream in sync transport"
                raise TypeError(err) from None
            return builder.body_stream(request.stream)

    def _map_response(self, builder: SyncRequestBuilder, request: httpx.Request) -> httpx.Response:
        req_exit_stack = ExitStack()
        try:
            response = req_exit_stack.enter_context(builder.build_streamed())
            return build_httpx_response(response, response_stream=SyncResponseStream(response, req_exit_stack))
        except Exception as exc:
            req_exit_stack.close()
            if isinstance(exc, PyreqwestError) and (mapped := map_exception(exc, request)):
                raise mapped from exc
            raise


class ResponseStream(httpx.AsyncByteStream):
    """httpx AsyncByteStream that wraps a pyreqwest Response body reader."""

    def __init__(self, response: Response, exit_stack: AsyncExitStack) -> None:
        """Internally initialized."""
        self._body_reader = response.body_reader
        self._exit_stack = exit_stack

    async def __aiter__(self) -> AsyncIterator[bytes]:
        """Asynchronously iterate over the response body in chunks."""
        while (chunk := await self._body_reader.read()) is not None:
            yield bytes(chunk)

    async def aclose(self) -> None:
        """Close the response stream."""
        await self._exit_stack.aclose()


class SyncResponseStream(httpx.SyncByteStream):
    """httpx SyncByteStream that wraps a pyreqwest SyncResponse body reader."""

    def __init__(self, response: SyncResponse, exit_stack: ExitStack) -> None:
        """Internally initialized."""
        self._body_reader = response.body_reader
        self._exit_stack = exit_stack

    def __iter__(self) -> Iterator[bytes]:
        """Iterate over the response body in chunks."""
        while (chunk := self._body_reader.read()) is not None:
            yield bytes(chunk)

    def close(self) -> None:
        """Close the response stream."""
        self._exit_stack.close()
