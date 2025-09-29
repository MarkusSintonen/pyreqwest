import argparse
import asyncio
import ssl
import statistics
import time
from collections.abc import AsyncGenerator, Callable, Coroutine, Iterator
from concurrent.futures import ThreadPoolExecutor
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any

import matplotlib.pyplot as plt
import trustme
from aiohttp import TCPConnector
from granian.constants import HTTPModes
from matplotlib.axes import Axes
from matplotlib.patches import Rectangle
from pyreqwest.client import ClientBuilder, SyncClientBuilder
from pyreqwest.http import Url

from tests.servers.echo_server import EchoServer
from tests.servers.server import EmbeddedServer, ServerConfig, find_free_port


class PerformanceBenchmark:
    """Benchmark class for comparing HTTP client performance."""

    def __init__(self, server_url: Url, comparison_lib: str, trust_cert_der: bytes) -> None:
        """Initialize benchmark with echo server and comparison library."""
        self.url = server_url.with_query({"echo_only_body": "1"})
        self.comparison_lib = comparison_lib
        self.is_sync = comparison_lib == "urllib3"
        self.trust_cert_der = trust_cert_der
        self.body_sizes = [
            10_000,  # 10KB
            100_000,  # 100KB
            1_000_000,  # 1MB
            5_000_000,  # 5MB
        ]
        self.big_body_limit = 1_000_000
        self.big_body_chunk_size = 1024 * 1024
        self.requests = 100
        self.concurrency_levels = [2, 10, 100]
        self.warmup_iterations = 5
        self.iterations = 50
        # Structure {client: {body_size: {concurrency: [times]}}}
        self.results: dict[str, dict[int, dict[int, list[float]]]] = {
            "pyreqwest": {},
            self.comparison_lib: {},
        }

    def generate_body(self, size: int) -> bytes:
        return b"x" * size

    async def meas_concurrent_batch(self, fn: Callable[[], Coroutine[Any, Any, None]], concurrency: int) -> list[float]:
        semaphore = asyncio.Semaphore(concurrency)

        async def run() -> float:
            async def sem_fn() -> None:
                async with semaphore:
                    await fn()

            start_time = time.perf_counter()
            await asyncio.gather(*(sem_fn() for _ in range(self.requests)))
            return (time.perf_counter() - start_time) * 1000

        print("    Warming up...")
        _ = [await run() for _ in range(self.warmup_iterations)]
        print("    Running benchmark...")
        return [await run() for _ in range(self.iterations)]

    def sync_meas_concurrent_batch(self, fn: Callable[[], None], concurrency: int) -> list[float]:
        with ThreadPoolExecutor(max_workers=concurrency) as executor:

            def run() -> float:
                start_time = time.perf_counter()
                futures = [executor.submit(fn) for _ in range(self.requests)]
                _ = [f.result() for f in futures]
                return (time.perf_counter() - start_time) * 1000

            print("    Warming up...")
            _ = [run() for _ in range(self.warmup_iterations)]
            print("    Running benchmark...")
            return [run() for _ in range(self.iterations)]

    def body_parts_sync(self, body: bytes) -> Iterator[bytes]:
        chunk_size = self.big_body_chunk_size
        for i in range(0, len(body), chunk_size):
            yield body[i : i + chunk_size]

    async def body_parts(self, body: bytes) -> AsyncGenerator[bytes, None]:
        for part in self.body_parts_sync(body):
            yield part

    async def benchmark_pyreqwest_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        body = self.generate_body(body_size)

        async with ClientBuilder().add_root_certificate_der(self.trust_cert_der).https_only(True).build() as client:

            async def post_read() -> None:
                if body_size <= self.big_body_limit:
                    response = await client.post(self.url).body_bytes(body).build().send()
                    assert len(await response.bytes()) == body_size
                else:
                    async with (
                        client.post(self.url)
                        .body_stream(self.body_parts(body))
                        .streamed_read_buffer_limit(65536 * 2)  # Same as aiohttp read buffer high watermark
                        .build_streamed() as response
                    ):
                        tot = 0
                        while chunk := await response.body_reader.read(self.big_body_chunk_size):
                            assert len(chunk) <= self.big_body_chunk_size
                            tot += len(chunk)
                        assert tot == body_size

            return await self.meas_concurrent_batch(post_read, concurrency)

    def benchmark_sync_pyreqwest_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        body = self.generate_body(body_size)

        with SyncClientBuilder().add_root_certificate_der(self.trust_cert_der).https_only(True).build() as client:

            def post_read() -> None:
                if body_size <= self.big_body_limit:
                    response = client.post(self.url).body_bytes(body).build().send()
                    assert len(response.bytes()) == body_size
                else:
                    with (
                        client.post(self.url)
                        .body_stream(self.body_parts_sync(body))
                        .streamed_read_buffer_limit(65536 * 2)  # Same as aiohttp read buffer high watermark
                        .build_streamed() as response
                    ):
                        tot = 0
                        while chunk := response.body_reader.read(self.big_body_chunk_size):
                            assert len(chunk) <= self.big_body_chunk_size
                            tot += len(chunk)
                        assert tot == body_size

            return self.sync_meas_concurrent_batch(post_read, concurrency)

    async def benchmark_aiohttp_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        import aiohttp

        body = self.generate_body(body_size)
        url_str = str(self.url)
        ssl_ctx = ssl.create_default_context(cadata=self.trust_cert_der)

        async with aiohttp.ClientSession(connector=TCPConnector(ssl=ssl_ctx, limit=concurrency)) as session:

            async def post_read() -> None:
                if body_size <= self.big_body_limit:
                    async with session.post(url_str, data=body) as response:
                        assert len(await response.read()) == body_size
                else:
                    async with session.post(url_str, data=self.body_parts(body)) as response:
                        tot = 0
                        async for chunk in response.content.iter_chunked(self.big_body_chunk_size):
                            assert len(chunk) <= self.big_body_chunk_size
                            tot += len(chunk)
                        assert tot == body_size

            return await self.meas_concurrent_batch(post_read, concurrency)

    async def benchmark_httpx_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        import httpx

        body = self.generate_body(body_size)
        url_str = str(self.url)
        ssl_ctx = ssl.create_default_context(cadata=self.trust_cert_der)

        async with httpx.AsyncClient(verify=ssl_ctx, limits=httpx.Limits(max_connections=concurrency)) as client:

            async def post_read() -> None:
                if body_size <= self.big_body_limit:
                    response = await client.post(url_str, content=body)
                    assert len(await response.aread()) == body_size
                else:
                    response = await client.post(url_str, content=self.body_parts(body))
                    tot = 0
                    async for chunk in response.aiter_bytes(self.big_body_chunk_size):
                        assert len(chunk) <= self.big_body_chunk_size
                        tot += len(chunk)
                    assert tot == body_size

            return await self.meas_concurrent_batch(post_read, concurrency)

    def benchmark_urllib3_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        import urllib3

        body = self.generate_body(body_size)
        url_str = str(self.url)
        ssl_ctx = ssl.create_default_context(cadata=self.trust_cert_der)

        with urllib3.PoolManager(maxsize=concurrency, ssl_context=ssl_ctx) as pool:
            if body_size <= self.big_body_limit:

                def post_read() -> None:
                    response = pool.request("POST", url_str, body=body)
                    assert response.status == 200
                    assert len(response.data) == body_size
            else:

                def post_read() -> None:
                    response = pool.request("POST", url_str, body=self.body_parts_sync(body), preload_content=False)
                    assert response.status == 200
                    tot = 0
                    while chunk := response.read(self.big_body_chunk_size):
                        assert len(chunk) <= self.big_body_chunk_size
                        tot += len(chunk)
                    assert tot == body_size
                    response.release_conn()

            return self.sync_meas_concurrent_batch(post_read, concurrency)

    async def benchmark_comparison_lib_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        """Dispatch to the appropriate benchmark method based on comparison library."""
        if self.comparison_lib == "aiohttp":
            return await self.benchmark_aiohttp_concurrent(body_size, concurrency)
        if self.comparison_lib == "httpx":
            return await self.benchmark_httpx_concurrent(body_size, concurrency)
        if self.comparison_lib == "urllib3":
            return self.benchmark_urllib3_concurrent(body_size, concurrency)
        raise ValueError(f"Unsupported comparison library: {self.comparison_lib}")

    async def run_benchmarks(self) -> None:
        """Run all benchmarks."""
        print("Starting performance benchmarks...")
        print(f"Comparing pyreqwest vs {self.comparison_lib}")
        print(f"Echo server URL: {self.url}")
        print(
            f"Body sizes: {
                [f'{size // 1000}KB' if size < 1_000_000 else f'{size // 1_000_000}MB' for size in self.body_sizes]
            }"
        )
        print(f"Concurrency levels: {self.concurrency_levels}")
        print(f"Warmup iterations: {self.warmup_iterations}")
        print(f"Benchmark iterations: {self.iterations}")
        print()

        for body_size in self.body_sizes:
            size_label = f"{body_size // 1000}KB" if body_size < 1_000_000 else f"{body_size // 1_000_000}MB"
            print(f"Benchmarking {size_label} body size...")

            # Initialize nested dictionaries for this body size
            self.results["pyreqwest"][body_size] = {}
            self.results[self.comparison_lib][body_size] = {}

            for concurrency in self.concurrency_levels:
                print(f"  Testing concurrency level: {concurrency}")

                if self.is_sync:
                    print("    Running sync pyreqwest benchmark...")
                    pyreqwest_times = self.benchmark_sync_pyreqwest_concurrent(body_size, concurrency)
                else:
                    print("    Running async pyreqwest benchmark...")
                    pyreqwest_times = await self.benchmark_pyreqwest_concurrent(body_size, concurrency)
                pyreqwest_avg = statistics.mean(pyreqwest_times)
                print(f"    pyreqwest average: {pyreqwest_avg:.4f}ms")
                self.results["pyreqwest"][body_size][concurrency] = pyreqwest_times

                print(f"    Running {self.comparison_lib} benchmark...")
                lib_times = await self.benchmark_comparison_lib_concurrent(body_size, concurrency)
                lib_avg = statistics.mean(lib_times)
                print(f"    {self.comparison_lib} average: {lib_avg:.4f}ms")
                self.results[self.comparison_lib][body_size][concurrency] = lib_times

                speedup = lib_avg / pyreqwest_avg if pyreqwest_avg != 0 else 0
                print(f"    Speedup: {speedup:.2f}x")
                print()

    def create_plot(self) -> None:
        """Create performance comparison plots."""
        # Create a grid layout - 4 rows * 3 columns for 12 subplots
        fig, axes = plt.subplots(nrows=len(self.body_sizes), ncols=len(self.concurrency_levels), figsize=(18, 16))
        fig.suptitle(f"pyreqwest vs {self.comparison_lib}", fontsize=16, y=0.98)
        legend_colors = {"pyreqwest": "lightblue", self.comparison_lib: "lightcoral"}

        for i, body_size in enumerate(self.body_sizes):
            size_label = f"{body_size // 1000}KB" if body_size < 1_000_000 else f"{body_size // 1_000_000}MB"
            ymax = 0.0

            for j, concurrency in enumerate(self.concurrency_levels):
                ax: Axes = axes[i][j]

                # Prepare data for this specific combination
                data_to_plot = [
                    self.results["pyreqwest"][body_size][concurrency],
                    self.results[self.comparison_lib][body_size][concurrency],
                ]

                # Create box plot for this specific body size and concurrency combination
                box_plot = ax.boxplot(
                    data_to_plot,
                    patch_artist=True,
                    showfliers=False,
                    tick_labels=["pyreqwest", self.comparison_lib],
                    widths=0.6,
                )
                ymax = max(ymax, ax.get_ylim()[1])

                # Color the boxes
                for patch, color in zip(box_plot["boxes"], legend_colors.values(), strict=False):
                    patch.set_facecolor(color)

                # Customize subplot
                ax.set_title(f"{size_label} @ {concurrency} concurrent", fontweight="bold", pad=10)
                ax.set_ylabel("Response Time (ms)")
                ax.grid(True, alpha=0.3)

                # Calculate and add performance comparison
                pyreqwest_median = statistics.median(self.results["pyreqwest"][body_size][concurrency])
                comparison_median = statistics.median(self.results[self.comparison_lib][body_size][concurrency])
                speedup = comparison_median / pyreqwest_median if pyreqwest_median != 0 else 0

                if speedup > 1:
                    faster_lib = "pyreqwest"
                    speedup_text = f"{((speedup - 1) * 100):.1f}% faster"
                else:
                    faster_lib = self.comparison_lib
                    speedup_text = f"{((1 / speedup - 1) * 100):.1f}% faster"

                # Add performance annotation
                ax.text(
                    0.5,
                    0.95,
                    f"{faster_lib}\n{speedup_text}",
                    transform=ax.transAxes,
                    ha="center",
                    va="top",
                    bbox={"boxstyle": "round,pad=0.3", "facecolor": "wheat", "alpha": 0.8},
                    fontsize=9,
                    fontweight="bold",
                )

                # Add median time annotations
                ax.text(
                    1,
                    pyreqwest_median,
                    f"{pyreqwest_median:.3f}ms",
                    ha="left",
                    va="center",
                    fontsize=8,
                    color="darkblue",
                    fontweight="bold",
                )
                ax.text(
                    2,
                    comparison_median,
                    f"{comparison_median:.3f}ms",
                    ha="right",
                    va="center",
                    fontsize=8,
                    color="darkred",
                    fontweight="bold",
                )

            for j, _ in enumerate(self.concurrency_levels):
                axes[i][j].set_ylim(ymin=0, ymax=ymax * 1.01)  # Uniform y-axis per row

        # Add overall legend
        legends = [
            Rectangle(xy=(0, 0), width=1, height=1, label=label, facecolor=color)
            for label, color in legend_colors.items()
        ]
        fig.legend(handles=legends, loc="lower center", bbox_to_anchor=(0.5, 0.01), ncol=2)

        plt.tight_layout()
        plt.subplots_adjust(top=0.94, bottom=0.06)  # Make room for suptitle and legend

        # Save the plot
        img_path = Path(__file__).parent / f"benchmark_{self.comparison_lib}.png"
        plt.savefig(str(img_path), dpi=300, bbox_inches="tight")
        print(f"Plot saved as '{img_path}'")


def cert_pem_to_der_bytes(cert_pem: bytes) -> bytes:
    return ssl.PEM_cert_to_DER_cert(cert_pem.decode())


@asynccontextmanager
async def server() -> AsyncGenerator[tuple[EmbeddedServer, bytes], None]:
    ca = trustme.CA()
    cert_der = ssl.PEM_cert_to_DER_cert(ca.cert_pem.bytes().decode())
    cert = ca.issue_cert("127.0.0.1", "localhost")
    with cert.cert_chain_pems[0].tempfile() as cert_tmp, cert.private_key_pem.tempfile() as pk_tmp:
        cert_file = Path(cert_tmp)
        pk_file = Path(pk_tmp)
        port = find_free_port()

        async with EmbeddedServer(
            EchoServer(), port, ServerConfig(ssl_key=pk_file, ssl_cert=cert_file, http=HTTPModes.http1)
        ).serve_context() as echo_server:
            yield echo_server, cert_der


async def main() -> None:
    parser = argparse.ArgumentParser(description="Performance benchmark")
    parser.add_argument("--lib", type=str, choices=["aiohttp", "httpx", "urllib3"], default="aiohttp")

    args = parser.parse_args()

    async with server() as (echo_server, trust_cert_der):
        benchmark = PerformanceBenchmark(echo_server.url, args.lib, trust_cert_der)
        await benchmark.run_benchmarks()
        benchmark.create_plot()


if __name__ == "__main__":
    asyncio.run(main())
