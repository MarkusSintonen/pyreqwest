"""Performance benchmark comparing pyreqwest and aiohttp with different body sizes and concurrency levels."""

import asyncio
import statistics
import time
from typing import Coroutine, Any, Callable, Iterable

import aiohttp
import matplotlib.pyplot as plt
from matplotlib.axes import Axes

from pyreqwest.client import ClientBuilder

from tests.servers.echo_server import EchoServer


class PerformanceBenchmark:
    """Benchmark class for comparing HTTP client performance."""

    def __init__(self, echo_server: EchoServer) -> None:
        """Initialize benchmark with echo server."""
        self.echo_server = echo_server
        self.url = echo_server.url.with_query({"echo_only_body": "1"})
        self.body_sizes = [
            1000,  # 1KB
            10_000,  # 10KB
            100_000,  # 100KB
            1_000_000,  # 1MB
        ]
        self.requests = 100
        self.concurrency_levels = [1, 10, 100]
        self.warmup_iterations = 20
        self.iterations = 100
        # Structure: {client: {body_size: {concurrency: [times]}}}
        self.results: dict[str, dict[int, dict[int, list[float]]]] = {
            "pyreqwest": {},
            "aiohttp": {},
        }

    def generate_body(self, size: int) -> bytes:
        """Generate test body of specified size."""
        return b"x" * size

    async def run_concurrency_limited(self, coros: Iterable[Coroutine[Any, Any, None]], concurrency: int) -> None:
        """Run tasks with a limit on concurrency."""
        semaphore = asyncio.Semaphore(concurrency)

        async def sem_task(coro: Coroutine[Any, Any, None]) -> None:
            async with semaphore:
                await coro

        await asyncio.gather(*(sem_task(coro) for coro in coros))

    async def meas_concurrent_batch(
        self, fn: Callable[[], Coroutine[Any, Any, None]], concurrency: int, timings: list[float]
    ) -> None:
        async def measured_fn() -> None:
            start_time = time.perf_counter()
            await fn()
            timings.append((time.perf_counter() - start_time) * 1000)

        await self.run_concurrency_limited((measured_fn() for _ in range(self.requests)), concurrency)

    async def benchmark_pyreqwest_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        """Benchmark pyreqwest with specified body size and concurrency."""
        body = self.generate_body(body_size)
        timings = []

        async with ClientBuilder().build() as client:
            async def post_read():
                response = await client.post(self.url).body_bytes(body).build_consumed().send()
                await response.bytes()

            # Warmup rounds
            print(f"    Warming up ({self.warmup_iterations} batches with {concurrency} concurrent requests)...")
            for _ in range(self.warmup_iterations):
                await self.meas_concurrent_batch(post_read, concurrency, [])

            # Actual benchmark rounds
            print(f"    Running benchmark ({self.iterations} batches with {concurrency} concurrent requests)...")
            for _ in range(self.iterations):
                await self.meas_concurrent_batch(post_read, concurrency, timings)

        return timings

    async def benchmark_aiohttp_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        """Benchmark aiohttp with specified body size and concurrency."""
        body = self.generate_body(body_size)
        url_str = str(self.url)
        timings = []

        async with aiohttp.ClientSession() as session:
            async def post_read():
                async with session.post(url_str, data=body) as response:
                    await response.read()

            # Warmup rounds
            print(f"    Warming up ({self.warmup_iterations} batches with {concurrency} concurrent requests)...")
            for _ in range(self.warmup_iterations):
                await self.meas_concurrent_batch(post_read, concurrency, [])

            # Actual benchmark rounds
            print(f"    Running benchmark ({self.iterations} batches with {concurrency} concurrent requests)...")
            for _ in range(self.iterations):
                await self.meas_concurrent_batch(post_read, concurrency, timings)

        return timings

    async def run_benchmarks(self) -> None:
        """Run all benchmarks."""
        print("Starting performance benchmarks...")
        print(f"Echo server URL: {self.url}")
        print(f"Body sizes: {[f'{size//1000}KB' if size < 1_000_000 else f'{size//1_000_000}MB' for size in self.body_sizes]}")
        print(f"Concurrency levels: {self.concurrency_levels}")
        print(f"Warmup iterations: {self.warmup_iterations}")
        print(f"Benchmark iterations: {self.iterations}")
        print()

        for body_size in self.body_sizes:
            size_label = f"{body_size//1000}KB" if body_size < 1_000_000 else f"{body_size//1_000_000}MB"
            print(f"Benchmarking {size_label} body size...")

            # Initialize nested dictionaries for this body size
            self.results["pyreqwest"][body_size] = {}
            self.results["aiohttp"][body_size] = {}

            for concurrency in self.concurrency_levels:
                print(f"  Testing concurrency level: {concurrency}")

                print(f"    Running pyreqwest benchmark...")
                pyreqwest_times = await self.benchmark_pyreqwest_concurrent(body_size, concurrency)
                self.results["pyreqwest"][body_size][concurrency] = pyreqwest_times

                print(f"    Running aiohttp benchmark...")
                aiohttp_times = await self.benchmark_aiohttp_concurrent(body_size, concurrency)
                self.results["aiohttp"][body_size][concurrency] = aiohttp_times

                # Print summary for this body size and concurrency level
                pyreqwest_avg = statistics.mean(pyreqwest_times)
                aiohttp_avg = statistics.mean(aiohttp_times)
                print(f"    pyreqwest average: {pyreqwest_avg:.4f}ms")
                print(f"    aiohttp average: {aiohttp_avg:.4f}ms")
                speedup = aiohttp_avg / pyreqwest_avg if pyreqwest_avg != 0 else 0
                print(f"    Speedup: {speedup:.2f}x")
                print()

    def create_plot(self) -> None:
        # Create a grid layout - 4 rows × 3 columns for 12 subplots
        fig, axes = plt.subplots(nrows=len(self.body_sizes), ncols=len(self.concurrency_levels), figsize=(18, 16))
        fig.suptitle("pyreqwest vs aiohttp", fontsize=16, y=0.98)
        legend_colors = {"pyreqwest": "lightblue", "aiohttp": "lightcoral"}

        for i, body_size in enumerate(self.body_sizes):
            size_label = f"{body_size//1000}KB" if body_size < 1_000_000 else f"{body_size//1_000_000}MB"

            for j, concurrency in enumerate(self.concurrency_levels):
                ax: Axes = axes[i][j]

                # Prepare data for this specific combination
                data_to_plot = [
                    self.results["pyreqwest"][body_size][concurrency],
                    self.results["aiohttp"][body_size][concurrency]
                ]

                # Create box plot for this specific body size and concurrency combination
                box_plot = ax.boxplot(
                    data_to_plot,
                    patch_artist=True,
                    showfliers=False,
                    tick_labels=[*legend_colors.keys()],
                    widths=0.6,
                )

                # Color the boxes
                for patch, color in zip(box_plot["boxes"], legend_colors.values()):
                    patch.set_facecolor(color)

                # Customize subplot
                ax.set_title(f"{size_label} @ {concurrency} concurrent", fontweight='bold', pad=10)
                ax.set_ylabel("Response Time (ms)")
                ax.grid(True, alpha=0.3)

                # Calculate and add performance comparison
                pyreqwest_median = statistics.median(self.results["pyreqwest"][body_size][concurrency])
                aiohttp_median = statistics.median(self.results["aiohttp"][body_size][concurrency])
                speedup = aiohttp_median / pyreqwest_median if pyreqwest_median != 0 else 0

                if speedup > 1:
                    faster_lib = "pyreqwest"
                    speedup_text = f"{((speedup - 1) * 100):.1f}% faster"
                else:
                    faster_lib = "aiohttp"
                    speedup_text = f"{((1/speedup - 1) * 100):.1f}% faster"

                # Add performance annotation
                ax.text(0.5, 0.95, f"{faster_lib}\n{speedup_text}",
                       transform=ax.transAxes, ha='center', va='top',
                       bbox=dict(boxstyle="round,pad=0.3", facecolor="wheat", alpha=0.8),
                       fontsize=9, fontweight='bold')

                # Add median time annotations
                ax.text(1, pyreqwest_median, f"{pyreqwest_median:.3f}ms",
                       ha='left', va='center', fontsize=8, color='darkblue', fontweight='bold')
                ax.text(2, aiohttp_median, f"{aiohttp_median:.3f}ms",
                       ha='right', va='center', fontsize=8, color='darkred', fontweight='bold')

        # Add overall legend
        legends = [
            plt.Rectangle(xy=(0, 0), width=1, height=1, label=label, facecolor=color)
            for label, color in legend_colors.items()
        ]
        fig.legend(handles=legends, loc='lower center', bbox_to_anchor=(0.5, 0.01), ncol=2)

        plt.tight_layout()
        plt.subplots_adjust(top=0.94, bottom=0.06)  # Make room for suptitle and legend

        # Save the plot
        plt.savefig("performance_benchmark_boxplot.png", dpi=300, bbox_inches="tight")
        print("Plot saved as 'performance_benchmark_boxplot.png'")


async def main() -> None:
    async with EchoServer().serve_context() as echo_server:
        benchmark = PerformanceBenchmark(echo_server)
        await benchmark.run_benchmarks()
        benchmark.create_plot()


if __name__ == "__main__":
    asyncio.run(main())
