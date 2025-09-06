import argparse
import asyncio
import ssl
import statistics
import time
from collections.abc import AsyncGenerator, Callable, Coroutine
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any

import matplotlib.pyplot as plt
import trustme
from aiohttp import TCPConnector
from granian.constants import HTTPModes
from matplotlib.axes import Axes
from matplotlib.patches import Rectangle
from pyreqwest.client import ClientBuilder
from pyreqwest.http import Url

from tests.servers.echo_server import EchoServer


class PerformanceBenchmark:
    """Benchmark class for comparing HTTP client performance."""

    def __init__(self, server_url: Url, comparison_lib: str, trust_cert_der: bytes) -> None:
        """Initialize benchmark with echo server and comparison library."""
        self.url = server_url.with_query({"echo_only_body": "1"})
        self.comparison_lib = comparison_lib
        self.trust_cert_der = trust_cert_der
        self.body_sizes = [
            10_000,  # 10KB
            100_000,  # 100KB
            1_000_000,  # 1MB
            10_000_000,  # 10MB
        ]
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
        """Generate test body of specified size."""
        return b"x" * size

    async def meas_concurrent_batch(
        self, fn: Callable[[], Coroutine[Any, Any, None]], concurrency: int, timings: list[float]
    ) -> None:
        semaphore = asyncio.Semaphore(concurrency)

        async def sem_task(coro: Coroutine[Any, Any, None]) -> None:
            async with semaphore:
                await coro

        start_time = time.perf_counter()
        await asyncio.gather(*(sem_task(fn()) for _ in range(self.requests)))
        timings.append((time.perf_counter() - start_time) * 1000)

    async def benchmark_pyreqwest_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        """Benchmark pyreqwest with specified body size and concurrency."""
        body = self.generate_body(body_size)
        timings: list[float] = []

        async with ClientBuilder().add_root_certificate_der(self.trust_cert_der).https_only(True).build() as client:

            async def post_read() -> None:
                if body_size <= 1_000_000:
                    response = await client.post(self.url).body_bytes(body).build_consumed().send()
                    assert len(await response.bytes()) == body_size
                else:
                    buffer_size = 65536 * 2  # Same as aiohttp read buffer high watermark
                    async with (
                        client.post(self.url)
                        .body_bytes(body)
                        .streamed_read_buffer_limit(buffer_size)
                        .build_streamed() as response
                    ):
                        tot = 0
                        while chunk := await response.read(1024 * 1024):
                            assert len(chunk) <= 1024 * 1024
                            tot += len(chunk)
                        assert tot == body_size

            print("    Warming up...")
            for _ in range(self.warmup_iterations):
                await self.meas_concurrent_batch(post_read, concurrency, [])

            print("    Running benchmark...")
            for _ in range(self.iterations):
                await self.meas_concurrent_batch(post_read, concurrency, timings)

        return timings

    async def benchmark_aiohttp_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        """Benchmark aiohttp with specified body size and concurrency."""
        import aiohttp

        body = self.generate_body(body_size)
        url_str = str(self.url)
        timings: list[float] = []
        ssl_ctx = ssl.create_default_context(cadata=self.trust_cert_der)

        async with aiohttp.ClientSession(connector=TCPConnector(ssl=ssl_ctx)) as session:

            async def post_read() -> None:
                if body_size <= 1_000_000:
                    async with session.post(url_str, data=body) as response:
                        assert len(await response.read()) == body_size
                else:
                    async with session.post(url_str, data=body) as response:
                        tot = 0
                        async for chunk in response.content.iter_chunked(1024 * 1024):
                            assert len(chunk) <= 1024 * 1024
                            tot += len(chunk)
                        assert tot == body_size

            print("    Warming up...")
            for _ in range(self.warmup_iterations):
                await self.meas_concurrent_batch(post_read, concurrency, [])

            print("    Running benchmark...")
            for _ in range(self.iterations):
                await self.meas_concurrent_batch(post_read, concurrency, timings)

        return timings

    async def benchmark_httpx_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        """Benchmark httpx with specified body size and concurrency."""
        import httpx

        body = self.generate_body(body_size)
        url_str = str(self.url)
        timings: list[float] = []
        ssl_ctx = ssl.create_default_context(cadata=self.trust_cert_der)

        async with httpx.AsyncClient(verify=ssl_ctx) as client:

            async def post_read() -> None:
                response = await client.post(url_str, content=body)
                assert len(await response.aread()) == body_size

            print("    Warming up...")
            for _ in range(self.warmup_iterations):
                await self.meas_concurrent_batch(post_read, concurrency, [])

            print("    Running benchmark...")
            for _ in range(self.iterations):
                await self.meas_concurrent_batch(post_read, concurrency, timings)

        return timings

    async def benchmark_comparison_lib_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        """Dispatch to the appropriate benchmark method based on comparison library."""
        if self.comparison_lib == "aiohttp":
            return await self.benchmark_aiohttp_concurrent(body_size, concurrency)
        if self.comparison_lib == "httpx":
            return await self.benchmark_httpx_concurrent(body_size, concurrency)
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

                print("    Running pyreqwest benchmark...")
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
async def server() -> AsyncGenerator[tuple[EchoServer, bytes], None]:
    ca = trustme.CA()
    cert_der = ssl.PEM_cert_to_DER_cert(ca.cert_pem.bytes().decode())
    cert = ca.issue_cert("127.0.0.1", "localhost")
    with cert.cert_chain_pems[0].tempfile() as cert_tmp, cert.private_key_pem.tempfile() as pk_tmp:
        cert_file = Path(cert_tmp)
        pk_file = Path(pk_tmp)

        async with EchoServer(ssl_key=pk_file, ssl_cert=cert_file, http=HTTPModes.http1).serve_context() as echo_server:
            yield echo_server, cert_der


async def main() -> None:
    parser = argparse.ArgumentParser(description="Performance benchmark")
    parser.add_argument("--lib", type=str, choices=["aiohttp", "httpx"], default="aiohttp")

    args = parser.parse_args()

    async with server() as (echo_server, trust_cert_der):
        benchmark = PerformanceBenchmark(echo_server.url, args.lib, trust_cert_der)
        await benchmark.run_benchmarks()
        benchmark.create_plot()


if __name__ == "__main__":
    asyncio.run(main())
