import asyncio
import ssl
import time
from collections.abc import AsyncGenerator, Awaitable, Callable
from concurrent.futures import ThreadPoolExecutor

from pyreqwest.client import ClientBuilder, SyncClientBuilder
from pyreqwest.http import Url

from tests.bench.server import CaCert
from tests.bench.utils import fmt_size


class Runner:
    def __init__(
        self,
        url: Url,
        ca_cert: CaCert,
        *,
        full_consume_size_limit: int,
        stream_write_chunk_size: int,
        stream_read_chunk_size: int,
        num_requests: int,
        warmup_iterations: int,
        iterations: int,
    ) -> None:
        self.url = url
        self.ca_cert = ca_cert
        self.full_consume_size_limit = full_consume_size_limit
        self.stream_write_chunk_size = stream_write_chunk_size
        self.stream_read_chunk_size = stream_read_chunk_size
        self.num_requests = num_requests
        self.warmup_iterations = warmup_iterations
        self.iterations = iterations

    async def run_lib(self, lib: str, body: bytes, concurrency: int) -> list[float]:  # noqa: PLR0911
        if lib == "pyreqwest_st":
            return await self.run_pyreqwest_concurrent(body, concurrency, multithreaded=False)
        if lib == "pyreqwest_mt":
            return await self.run_pyreqwest_concurrent(body, concurrency, multithreaded=True)
        if lib == "pyreqwest_sync_st":
            return self.run_sync_pyreqwest_concurrent(body, concurrency, multithreaded=False)
        if lib == "pyreqwest_sync_mt":
            return self.run_sync_pyreqwest_concurrent(body, concurrency, multithreaded=True)
        if lib == "urllib3":
            return self.run_urllib3_concurrent(body, concurrency)
        if lib == "aiohttp":
            return await self.run_aiohttp_concurrent(body, concurrency)
        if lib == "httpx":
            return await self.run_httpx_concurrent(body, concurrency)
        if lib == "rnet":
            return await self.run_rnet_concurrent(body, concurrency)
        if lib == "niquests":
            return await self.run_niquests_concurrent(body, concurrency)
        if lib == "ry":
            return await self.run_ry_concurrent(body, concurrency)
        if lib == "curl_cffi":
            return await self.run_curl_cffi_concurrent(body, concurrency)
        raise ValueError(f"Unsupported comparison library: {lib}")

    async def meas_concurrent_batch(
        self, fn: Callable[[], Awaitable[None]], body_size: int, concurrency: int
    ) -> list[float]:
        print(f"Benchmarking, body_size={body_size}, concurrency={concurrency}", flush=True)
        semaphore = asyncio.Semaphore(concurrency)

        async def run() -> float:
            async def sem_fn() -> None:
                async with semaphore:
                    await fn()

            print(".", end="", flush=True)
            start_time = time.perf_counter()
            await asyncio.gather(*(sem_fn() for _ in range(self.num_requests)))
            return (time.perf_counter() - start_time) * 1000

        _ = [await run() for _ in range(self.warmup_iterations)]
        res = [await run() for _ in range(self.iterations)]
        print(flush=True)
        return res

    def sync_meas_concurrent_batch(self, fn: Callable[[], None], body_size: int, concurrency: int) -> list[float]:
        print(f"Benchmarking, body_size={fmt_size(body_size)}, concurrency={concurrency}", flush=True)
        with ThreadPoolExecutor(max_workers=concurrency) as executor:

            def run() -> float:
                print(".", end="", flush=True)
                start_time = time.perf_counter()
                futures = [executor.submit(fn) for _ in range(self.num_requests)]
                _ = [f.result() for f in futures]
                return (time.perf_counter() - start_time) * 1000

            _ = [run() for _ in range(self.warmup_iterations)]
            res = [run() for _ in range(self.iterations)]
        print(flush=True)
        return res

    def body_parts_chunks(self, body: bytes) -> list[bytes]:
        chunk_size = self.stream_write_chunk_size
        return [body[i : i + chunk_size] for i in range(0, len(body), chunk_size)]

    async def body_parts_stream(self, chunks: list[bytes]) -> AsyncGenerator[bytes, str]:
        for part in chunks:
            await asyncio.sleep(0)
            yield part

    async def run_pyreqwest_concurrent(self, body: bytes, concurrency: int, multithreaded: bool) -> list[float]:
        async with (
            ClientBuilder()
            .add_root_certificate_der(self.ca_cert.der)
            .runtime_multithreaded(multithreaded)
            .https_only(True)
            .build() as client
        ):
            if len(body) <= self.full_consume_size_limit:

                async def post_read() -> None:
                    response = await client.post(self.url).body_bytes(body).build().send()
                    assert len(await response.bytes()) == len(body)
            else:
                chunks = self.body_parts_chunks(body)

                async def post_read() -> None:
                    async with (
                        client.post(self.url)
                        .body_stream(self.body_parts_stream(chunks))
                        .streamed_read_buffer_limit(65536 * 2)  # Same as aiohttp read buffer high watermark
                        .build_streamed() as response
                    ):
                        tot = 0
                        while chunk := await response.body_reader.read(self.stream_read_chunk_size):
                            assert len(chunk) <= self.stream_read_chunk_size
                            tot += len(chunk)
                        assert tot == len(body)

            return await self.meas_concurrent_batch(post_read, len(body), concurrency)

    def run_sync_pyreqwest_concurrent(self, body: bytes, concurrency: int, multithreaded: bool) -> list[float]:
        with (
            SyncClientBuilder()
            .add_root_certificate_der(self.ca_cert.der)
            .runtime_multithreaded(multithreaded)
            .https_only(True)
            .build() as client
        ):
            if len(body) <= self.full_consume_size_limit:

                def post_read() -> None:
                    response = client.post(self.url).body_bytes(body).build().send()
                    assert len(response.bytes()) == len(body)
            else:
                chunks = self.body_parts_chunks(body)

                def post_read() -> None:
                    with (
                        client.post(self.url)
                        .body_stream(iter(chunks))
                        .streamed_read_buffer_limit(65536 * 2)  # Same as aiohttp read buffer high watermark
                        .build_streamed() as response
                    ):
                        tot = 0
                        while chunk := response.body_reader.read(self.stream_read_chunk_size):
                            assert len(chunk) <= self.stream_read_chunk_size
                            tot += len(chunk)
                        assert tot == len(body)

            return self.sync_meas_concurrent_batch(post_read, len(body), concurrency)

    async def run_aiohttp_concurrent(self, body: bytes, concurrency: int) -> list[float]:
        import aiohttp

        url_str = str(self.url)
        ssl_ctx = ssl.create_default_context(cadata=self.ca_cert.der)

        async with aiohttp.ClientSession(connector=aiohttp.TCPConnector(ssl=ssl_ctx, limit=concurrency)) as session:
            if len(body) <= self.full_consume_size_limit:

                async def post_read() -> None:
                    async with session.post(url_str, data=body) as response:
                        assert len(await response.read()) == len(body)
            else:
                chunks = self.body_parts_chunks(body)

                async def post_read() -> None:
                    async with session.post(url_str, data=self.body_parts_stream(chunks)) as response:
                        tot = 0
                        async for chunk in response.content.iter_chunked(self.stream_read_chunk_size):
                            assert len(chunk) <= self.stream_read_chunk_size
                            tot += len(chunk)
                        assert tot == len(body)

            return await self.meas_concurrent_batch(post_read, len(body), concurrency)

    async def run_httpx_concurrent(self, body: bytes, concurrency: int) -> list[float]:
        import httpx

        url_str = str(self.url)
        ssl_ctx = ssl.create_default_context(cadata=self.ca_cert.der)

        async with httpx.AsyncClient(verify=ssl_ctx, limits=httpx.Limits(max_connections=concurrency)) as client:
            if len(body) <= self.full_consume_size_limit:

                async def post_read() -> None:
                    response = await client.post(url_str, content=body)
                    assert len(await response.aread()) == len(body)
            else:
                chunks = self.body_parts_chunks(body)

                async def post_read() -> None:
                    response = await client.post(url_str, content=self.body_parts_stream(chunks))
                    tot = 0
                    async for chunk in response.aiter_bytes(self.stream_read_chunk_size):
                        assert len(chunk) <= self.stream_read_chunk_size
                        tot += len(chunk)
                    assert tot == len(body)

            return await self.meas_concurrent_batch(post_read, len(body), concurrency)

    def run_urllib3_concurrent(self, body: bytes, concurrency: int) -> list[float]:
        import urllib3

        url_str = str(self.url)
        ssl_ctx = ssl.create_default_context(cadata=self.ca_cert.der)

        with urllib3.PoolManager(maxsize=concurrency, ssl_context=ssl_ctx) as pool:
            if len(body) <= self.full_consume_size_limit:

                def post_read() -> None:
                    response = pool.request("POST", url_str, body=body)
                    assert response.status == 200
                    assert len(response.data) == len(body)
            else:
                chunks = self.body_parts_chunks(body)

                def post_read() -> None:
                    response = pool.request("POST", url_str, body=iter(chunks), preload_content=False)
                    assert response.status == 200
                    tot = 0
                    while chunk := response.read(self.stream_read_chunk_size):
                        assert len(chunk) <= self.stream_read_chunk_size
                        tot += len(chunk)
                    assert tot == len(body)
                    response.release_conn()

            return self.sync_meas_concurrent_batch(post_read, len(body), concurrency)

    async def run_rnet_concurrent(self, body: bytes, concurrency: int) -> list[float]:
        import rnet

        url_str = str(self.url)
        client = rnet.Client(verify=False, https_only=True)

        if len(body) <= self.full_consume_size_limit:

            async def post_read() -> None:
                response = await client.post(url_str, body=body)  # noqa: F821
                async with response as response:
                    assert response.status == 200
                    assert len(await response.bytes()) == len(body)
        else:
            chunks = self.body_parts_chunks(body)

            async def post_read() -> None:
                response = await client.post(url_str, body=self.body_parts_stream(chunks))  # noqa: F821
                async with response as response:
                    assert response.status == 200
                    tot = 0
                    async with response.stream() as streamer:
                        async for chunk in streamer:
                            tot += len(chunk)
                    assert tot == len(body)

        res = await self.meas_concurrent_batch(post_read, len(body), concurrency)
        del client  # No close or context manager in rnet. Make sure to dispose anything now.
        return res

    async def run_ry_concurrent(self, body: bytes, concurrency: int) -> list[float]:
        import ry  # type: ignore[import-not-found]

        url_str = str(self.url)
        # for fairness w/ pyreqwest which uses its `pyreqwest.Url` wrapper, use the ry.URL struct
        _url_ob = ry.URL(url_str)
        client = ry.Client(
            https_only=True,
            tls_certs_merge=[ry.Certificate.from_der(self.ca_cert.der)],
        )
        if len(body) <= self.full_consume_size_limit:

            async def post_read() -> None:
                response = await client.post(_url_ob, body=body)  # noqa: F821
                assert response.status == 200
                assert len(await response.bytes()) == len(body)
        else:
            chunks = self.body_parts_chunks(body)

            async def post_read() -> None:
                response = await client.post(_url_ob, body=self.body_parts_stream(chunks))  # noqa: F821
                assert response.status == 200
                tot = 0
                async for chunk in response.stream(self.stream_read_chunk_size):
                    tot += len(chunk)
                assert tot == len(body)

        res = await self.meas_concurrent_batch(post_read, len(body), concurrency)
        del client
        return res

    async def run_niquests_concurrent(self, body: bytes, concurrency: int) -> list[float]:
        import niquests

        url_str = str(self.url)
        async with niquests.AsyncSession(pool_connections=concurrency, pool_maxsize=concurrency) as client:
            client.verify = self.ca_cert.pem

            if len(body) <= self.full_consume_size_limit:

                async def post_read() -> None:
                    response = await client.post(url_str, data=body)
                    assert isinstance(response, niquests.Response)  # niquests is weird...
                    assert response.status_code == 200
                    assert len(response.content or b"") == len(body)
                    response.close()
            else:
                chunks = self.body_parts_chunks(body)

                async def post_read() -> None:
                    response = await client.post(url_str, data=self.body_parts_stream(chunks), stream=True)
                    assert isinstance(response, niquests.AsyncResponse)  # niquests is weird...
                    assert response.status_code == 200
                    tot = 0
                    resp_iter = await response.iter_raw(self.stream_read_chunk_size)
                    async for chunk in resp_iter:
                        tot += len(chunk)
                    assert tot == len(body)

            return await self.meas_concurrent_batch(post_read, len(body), concurrency)

    async def run_curl_cffi_concurrent(self, body: bytes, concurrency: int) -> list[float]:
        import curl_cffi

        url_str = str(self.url)

        # No support for custom CA certs, so verify=False
        async with curl_cffi.AsyncSession(verify=False, max_clients=concurrency) as session:
            if len(body) <= self.full_consume_size_limit:

                async def post_read() -> None:
                    response = await session.post(url_str, data=body)
                    assert response.status_code == 200
                    assert len(response.content) == len(body)

                return await self.meas_concurrent_batch(post_read, len(body), concurrency)
            # curl_cffi is very limited, so can not benchmark compare large bodies streaming:
            # - no support for streaming request bodies
            # - no support for chunk size limits on response body streaming
            return []
