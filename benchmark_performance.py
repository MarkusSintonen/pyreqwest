"""Performance benchmark comparing pyreqwest and aiohttp with different body sizes and concurrency levels."""

import asyncio
import statistics
import time
from typing import Coroutine, Any

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

    async def benchmark_pyreqwest_concurrent(self, body_size: int, concurrency: int) -> list[float]:
        """Benchmark pyreqwest with specified body size and concurrency."""
        body = self.generate_body(body_size)
        times = []

        async with ClientBuilder().build() as client:
            async def post_read():
                response = await client.post(self.url).body_bytes(body).build_consumed().send()
                await response.bytes()

            async def run_concurrent_batch():
                """Run a batch of concurrent requests and measure total time."""
                start_time = time.perf_counter()
                tasks = [post_read() for _ in range(self.requests)]
                await self.run_concurrency_limited(tasks, concurrency)
                end_time = time.perf_counter()
                return (end_time - start_time) * 1000  # Convert to ms

            # Warmup rounds
            print(f"    Warming up ({self.warmup_iterations} batches of {concurrency} concurrent requests)...")
            for _ in range(self.warmup_iterations):
                await run_concurrent_batch()

            # Actual benchmark rounds
            print(f"    Running benchmark ({self.iterations} batches of {concurrency} concurrent requests)...")
            for _ in range(self.iterations):
                batch_time = await run_concurrent_batch()
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

            async def run_concurrent_batch():
                """Run a batch of concurrent requests and measure total time."""
                start_time = time.perf_counter()
                tasks = [post_read() for _ in range(self.requests)]
                await self.run_concurrency_limited(tasks, concurrency)
                end_time = time.perf_counter()
                return (end_time - start_time) * 1000  # Convert to ms

            # Warmup rounds
            print(f"    Warming up ({self.warmup_iterations} batches of {concurrency} concurrent requests)...")
            for _ in range(self.warmup_iterations):
                await run_concurrent_batch()

            # Actual benchmark rounds
            print(f"    Running benchmark ({self.iterations} batches of {concurrency} concurrent requests)...")
            for _ in range(self.iterations):
                batch_time = await run_concurrent_batch()
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

    def create_comprehensive_plot(self) -> None:
        """Create comprehensive plot showing performance comparison across body sizes and concurrency levels."""
        # Create a larger figure with subplots for each body size
        fig, axes = plt.subplots(2, 2, figsize=(16, 12))
        fig.suptitle("HTTP Client Performance: pyreqwest vs aiohttp\n(Body Sizes × Concurrency Levels)",
                     fontsize=16, y=0.98)

        # Flatten axes for easier iteration
        axes = axes.flatten()

        for i, body_size in enumerate(self.body_sizes):
            ax = axes[i]
            size_label = f"{body_size//1000}KB" if body_size < 1_000_000 else f"{body_size//1_000_000}MB"

            # Prepare data for grouped box plot
            positions = []
            data_to_plot = []
            labels = []
            colors = []

            for j, concurrency in enumerate(self.concurrency_levels):
                # Position calculation for grouped boxes
                base_pos = j * 3  # Space between concurrency groups

                # pyreqwest data
                positions.append(base_pos)
                data_to_plot.append(self.results["pyreqwest"][body_size][concurrency])
                labels.append(f"pyreqwest\n(c={concurrency})")
                colors.append("lightblue")

                # aiohttp data
                positions.append(base_pos + 1)
                data_to_plot.append(self.results["aiohttp"][body_size][concurrency])
                labels.append(f"aiohttp\n(c={concurrency})")
                colors.append("lightcoral")

            # Create box plot
            box_plot = ax.boxplot(
                data_to_plot,
                positions=positions,
                patch_artist=True,
                widths=0.7,
            )

            # Color the boxes
            for patch, color in zip(box_plot["boxes"], colors):
                patch.set_facecolor(color)

            # Customize subplot
            ax.set_title(f"{size_label} Body Size", fontweight='bold', pad=20)
            ax.set_ylabel("Response Time (ms)")
            ax.set_xlabel("Client (Concurrency Level)")
            ax.grid(True, alpha=0.3)

            # Set x-tick labels
            tick_positions = [pos + 0.5 for pos in range(0, len(self.concurrency_levels) * 3, 3)]
            ax.set_xticks(tick_positions)
            ax.set_xticklabels([f"c={c}" for c in self.concurrency_levels])

            # Add performance annotations for each concurrency level
            for j, concurrency in enumerate(self.concurrency_levels):
                pyreqwest_avg = statistics.mean(self.results["pyreqwest"][body_size][concurrency])
                aiohttp_avg = statistics.mean(self.results["aiohttp"][body_size][concurrency])
                speedup = aiohttp_avg / pyreqwest_avg if pyreqwest_avg != 0 else 0

                base_pos = j * 3
                if speedup > 1:
                    speedup_text = f"{speedup:.1f}x"
                    text_color = "blue"
                else:
                    speedup_text = f"{1/speedup:.1f}x"
                    text_color = "red"

                # Add speedup text above the boxes
                max_y = max(max(self.results["pyreqwest"][body_size][concurrency]),
                           max(self.results["aiohttp"][body_size][concurrency]))
                ax.text(base_pos + 0.5, max_y * 1.1, speedup_text,
                       ha='center', va='bottom', fontweight='bold',
                       color=text_color, fontsize=10)

        # Add overall legend
        blue_patch = plt.Rectangle((0, 0), 1, 1, facecolor="lightblue", label="pyreqwest")
        coral_patch = plt.Rectangle((0, 0), 1, 1, facecolor="lightcoral", label="aiohttp")
        fig.legend(handles=[blue_patch, coral_patch], loc='lower center',
                  bbox_to_anchor=(0.5, 0.02), ncol=2)

        plt.tight_layout()
        plt.subplots_adjust(top=0.93, bottom=0.12)  # Make room for suptitle and legend

        # Save the plot
        plt.savefig("performance_benchmark_boxplot.png", bbox_inches="tight")
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
        benchmark.create_comprehensive_plot()
        benchmark.print_summary()


if __name__ == "__main__":
    asyncio.run(main())
