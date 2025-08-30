"""Performance benchmark comparing pyreqwest and aiohttp with different body sizes and concurrency levels."""

import asyncio
import statistics
import time
from typing import Coroutine, Any, Callable

import aiohttp
import matplotlib.pyplot as plt
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

    async def run_concurrency_limited(self, coros: list[Coroutine[Any, Any, None]], concurrency: int) -> None:
        """Run tasks with a limit on concurrency."""
        semaphore = asyncio.Semaphore(concurrency)

        async def sem_task(coro: Coroutine[Any, Any, None]) -> None:
            async with semaphore:
                await coro

        await asyncio.gather(*(sem_task(coro) for coro in coros))

    async def meas_concurrent_batch(self, fn: Callable[[], Coroutine[Any, Any, None]], concurrency: int) -> float:
        start_time = time.perf_counter()
        tasks = [fn() for _ in range(self.requests)]
        await self.run_concurrency_limited(tasks, concurrency)
        end_time = time.perf_counter()
        return (end_time - start_time) * 1000  # Convert to ms

    async def benchmark_pyreqwest_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        """Benchmark pyreqwest with specified body size and concurrency."""
        body = self.generate_body(body_size)
        times = []

        async with ClientBuilder().build() as client:
            async def post_read():
                response = await client.post(self.url).body_bytes(body).build_consumed().send()
                await response.bytes()

            # Warmup rounds
            print(f"    Warming up ({self.warmup_iterations} batches with {concurrency} concurrent requests)...")
            for _ in range(self.warmup_iterations):
                await self.meas_concurrent_batch(post_read, concurrency)

            # Actual benchmark rounds
            print(f"    Running benchmark ({self.iterations} batches with {concurrency} concurrent requests)...")
            for _ in range(self.iterations):
                batch_time = await self.meas_concurrent_batch(post_read, concurrency)
                # Calculate average time per request in the batch
                avg_time_per_request = batch_time / concurrency
                times.append(avg_time_per_request)

        return times

    async def benchmark_aiohttp_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        """Benchmark aiohttp with specified body size and concurrency."""
        body = self.generate_body(body_size)
        url_str = str(self.url)
        times = []

        async with aiohttp.ClientSession() as session:
            async def post_read():
                async with session.post(url_str, data=body) as response:
                    await response.read()

            # Warmup rounds
            print(f"    Warming up ({self.warmup_iterations} batches with {concurrency} concurrent requests)...")
            for _ in range(self.warmup_iterations):
                await self.meas_concurrent_batch(post_read, concurrency)

            # Actual benchmark rounds
            print(f"    Running benchmark ({self.iterations} batches with {concurrency} concurrent requests)...")
            for _ in range(self.iterations):
                batch_time = await self.meas_concurrent_batch(post_read, concurrency)
                # Calculate average time per request in the batch
                avg_time_per_request = batch_time / concurrency
                times.append(avg_time_per_request)

        return times

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
        fig, axes = plt.subplots(4, 3, figsize=(18, 16))
        fig.suptitle("HTTP Client Performance: pyreqwest vs aiohttp\n(Separate Graphs for Each Body Size × Concurrency Combination)",
                     fontsize=16, y=0.98)

        # Flatten axes for easier iteration
        axes = axes.flatten()

        plot_index = 0
        for i, body_size in enumerate(self.body_sizes):
            size_label = f"{body_size//1000}KB" if body_size < 1_000_000 else f"{body_size//1_000_000}MB"

            for j, concurrency in enumerate(self.concurrency_levels):
                ax = axes[plot_index]

                # Prepare data for this specific combination
                data_to_plot = [
                    self.results["pyreqwest"][body_size][concurrency],
                    self.results["aiohttp"][body_size][concurrency]
                ]
                labels = ["pyreqwest", "aiohttp"]
                colors = ["lightblue", "lightcoral"]

                # Create box plot for this specific body size and concurrency combination
                box_plot = ax.boxplot(
                    data_to_plot,
                    patch_artist=True,
                    tick_labels=labels,
                    widths=0.6,
                )

                # Color the boxes
                for patch, color in zip(box_plot["boxes"], colors):
                    patch.set_facecolor(color)

                # Customize subplot
                ax.set_title(f"{size_label} @ {concurrency} concurrent", fontweight='bold', pad=10)
                ax.set_ylabel("Response Time (ms)")
                ax.grid(True, alpha=0.3)

                # Calculate and add performance comparison
                pyreqwest_avg = statistics.mean(self.results["pyreqwest"][body_size][concurrency])
                aiohttp_avg = statistics.mean(self.results["aiohttp"][body_size][concurrency])
                speedup = aiohttp_avg / pyreqwest_avg if pyreqwest_avg != 0 else 0

                if speedup > 1:
                    faster_lib = "pyreqwest"
                    speedup_text = f"{speedup:.2f}x faster"
                else:
                    faster_lib = "aiohttp"
                    speedup_text = f"{1/speedup:.2f}x faster"

                # Add performance annotation
                ax.text(0.5, 0.95, f"{faster_lib}\n{speedup_text}",
                       transform=ax.transAxes, ha='center', va='top',
                       bbox=dict(boxstyle="round,pad=0.3", facecolor="wheat", alpha=0.8),
                       fontsize=9, fontweight='bold')

                # Add average time annotations
                ax.text(1, pyreqwest_avg, f"{pyreqwest_avg:.3f}ms",
                       ha='left', va='center', fontsize=8, color='blue', fontweight='bold')
                ax.text(2, aiohttp_avg, f"{aiohttp_avg:.3f}ms",
                       ha='right', va='center', fontsize=8, color='red', fontweight='bold')

                plot_index += 1

        # Hide any unused subplots
        for i in range(plot_index, len(axes)):
            axes[i].set_visible(False)

        # Add overall legend
        blue_patch = plt.Rectangle((0, 0), 1, 1, facecolor="lightblue", label="pyreqwest")
        coral_patch = plt.Rectangle((0, 0), 1, 1, facecolor="lightcoral", label="aiohttp")
        fig.legend(handles=[blue_patch, coral_patch], loc='lower center',
                  bbox_to_anchor=(0.5, 0.01), ncol=2)

        plt.tight_layout()
        plt.subplots_adjust(top=0.94, bottom=0.06)  # Make room for suptitle and legend

        # Save the plot
        plt.savefig("performance_benchmark_boxplot.png", dpi=300, bbox_inches="tight")
        print("Comprehensive plot saved as 'performance_benchmark_boxplot.png'")

    def print_summary(self) -> None:
        """Print comprehensive benchmark summary."""
        print("\n" + "="*80)
        print("PERFORMANCE BENCHMARK SUMMARY")
        print("="*80)

        for body_size in self.body_sizes:
            size_label = f"{body_size//1000}KB" if body_size < 1_000_000 else f"{body_size//1_000_000}MB"
            print(f"\n{size_label} Body Size:")
            print("-" * 40)

            for concurrency in self.concurrency_levels:
                pyreqwest_times = self.results["pyreqwest"][body_size][concurrency]
                aiohttp_times = self.results["aiohttp"][body_size][concurrency]

                pyreqwest_stats = {
                    "mean": statistics.mean(pyreqwest_times),
                    "median": statistics.median(pyreqwest_times),
                    "stdev": statistics.stdev(pyreqwest_times),
                    "min": min(pyreqwest_times),
                    "max": max(pyreqwest_times),
                }

                aiohttp_stats = {
                    "mean": statistics.mean(aiohttp_times),
                    "median": statistics.median(aiohttp_times),
                    "stdev": statistics.stdev(aiohttp_times),
                    "min": min(aiohttp_times),
                    "max": max(aiohttp_times),
                }

                print(f"\n  Concurrency Level: {concurrency}")
                print(f"    pyreqwest - Mean: {pyreqwest_stats['mean']:.4f}ms, "
                      f"Median: {pyreqwest_stats['median']:.4f}ms, "
                      f"StdDev: {pyreqwest_stats['stdev']:.4f}ms")
                print(f"    aiohttp   - Mean: {aiohttp_stats['mean']:.4f}ms, "
                      f"Median: {aiohttp_stats['median']:.4f}ms, "
                      f"StdDev: {aiohttp_stats['stdev']:.4f}ms")

                speedup = aiohttp_stats['mean'] / pyreqwest_stats['mean'] if pyreqwest_stats['mean'] != 0 else 0
                if speedup > 1:
                    print(f"    Winner: pyreqwest ({speedup:.2f}x faster)")
                else:
                    print(f"    Winner: aiohttp ({1/speedup:.2f}x faster)")

        # Overall summary
        print(f"\n{'='*80}")
        print("OVERALL PERFORMANCE TRENDS")
        print("="*80)

        for concurrency in self.concurrency_levels:
            pyreqwest_wins = 0
            aiohttp_wins = 0

            for body_size in self.body_sizes:
                pyreqwest_avg = statistics.mean(self.results["pyreqwest"][body_size][concurrency])
                aiohttp_avg = statistics.mean(self.results["aiohttp"][body_size][concurrency])

                if pyreqwest_avg < aiohttp_avg:
                    pyreqwest_wins += 1
                else:
                    aiohttp_wins += 1

            print(f"\nConcurrency Level {concurrency}:")
            print(f"  pyreqwest wins: {pyreqwest_wins}/{len(self.body_sizes)} body sizes")
            print(f"  aiohttp wins: {aiohttp_wins}/{len(self.body_sizes)} body sizes")


async def main() -> None:
    """Main benchmark function."""
    # Start echo server using context manager
    async with EchoServer().serve_context() as echo_server:
        # Wait a moment for server to be ready
        await asyncio.sleep(0.1)

        # Run benchmarks
        benchmark = PerformanceBenchmark(echo_server)
        await benchmark.run_benchmarks()

        # Create visualizations and print summary
        benchmark.create_plot()
        benchmark.print_summary()


if __name__ == "__main__":
    asyncio.run(main())
