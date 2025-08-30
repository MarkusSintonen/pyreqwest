"""Performance benchmark comparing pyreqwest and aiohttp with different body sizes."""

import asyncio
import statistics
import time

import aiohttp
import matplotlib.pyplot as plt
from pyreqwest.client import ClientBuilder

from tests.servers.echo_server import EchoServer


class PerformanceBenchmark:
    """Benchmark class for comparing HTTP client performance."""

    def __init__(self, echo_server: EchoServer) -> None:
        """Initialize benchmark with echo server."""
        self.echo_server = echo_server
        self.body_sizes = [
            1000,  # 1KB
            10_000,  # 10KB
            100_000,  # 100KB
            1_000_000,  # 1MB
            10_000_000,  # 10MB
        ]
        self.warmup_iterations = 50  # Number of warmup requests
        self.iterations = 400  # Number of requests per test
        self.results: dict[str, dict[int, list[float]]] = {
            "pyreqwest": {},
            "aiohttp": {},
        }

    def generate_body(self, size: int) -> bytes:
        """Generate test body of specified size."""
        return b"x" * size

    async def benchmark_pyreqwest(self, body_size: int) -> list[float]:
        """Benchmark pyreqwest with specified body size."""
        times = []
        body = self.generate_body(body_size)

        async with ClientBuilder().build() as client:
            async def post_next_chunks():
                response = await client.post(self.echo_server.url).body_bytes(body).build_consumed().send()
                while await response.next_chunk() is not None:
                    pass

            async def post_read():
                response = await client.post(self.echo_server.url).body_bytes(body).build_consumed().send()
                await response.bytes()

            # Warmup rounds
            print(f"    Warming up ({self.warmup_iterations} requests)...")
            for _ in range(self.warmup_iterations):
                await post_read()

            # Actual benchmark rounds
            print(f"    Running benchmark ({self.iterations} requests)...")
            for _ in range(self.iterations):
                start_time = time.perf_counter()
                await post_read()
                end_time = time.perf_counter()
                times.append(end_time - start_time)

        return times

    async def benchmark_aiohttp(self, body_size: int) -> list[float]:
        """Benchmark aiohttp with specified body size."""
        times = []
        body = self.generate_body(body_size)
        url_str = str(self.echo_server.url)  # Convert pyreqwest URL to string

        async with aiohttp.ClientSession() as session:
            async def post_next_chunks():
                async with session.post(url_str, data=body) as response:
                    async for data, _ in response.content.iter_chunks():
                        pass

            async def post_read():
                async with session.post(url_str, data=body) as response:
                    await response.read()

            # Warmup rounds
            print(f"    Warming up ({self.warmup_iterations} requests)...")
            for _ in range(self.warmup_iterations):
                await post_read()

            # Actual benchmark rounds
            print(f"    Running benchmark ({self.iterations} requests)...")
            for _ in range(self.iterations):
                start_time = time.perf_counter()
                await post_read()
                end_time = time.perf_counter()
                times.append(end_time - start_time)

        return times

    async def run_benchmarks(self) -> None:
        """Run all benchmarks."""
        print("Starting performance benchmarks...")
        print(f"Echo server URL: {self.echo_server.url}")
        print(f"Body sizes: {[f'{size//1000}KB' if size < 1_000_000 else f'{size//1_000_000}MB' for size in self.body_sizes]}")
        print(f"Warmup iterations: {self.warmup_iterations}")
        print(f"Benchmark iterations: {self.iterations}")
        print()

        for body_size in self.body_sizes:
            size_label = f"{body_size//1000}KB" if body_size < 1_000_000 else f"{body_size//1_000_000}MB"
            print(f"Benchmarking {size_label} body size...")

            # Benchmark pyreqwest
            print("  Running pyreqwest benchmark...")
            pyreqwest_times = await self.benchmark_pyreqwest(body_size)
            self.results["pyreqwest"][body_size] = pyreqwest_times

            # Benchmark aiohttp
            print("  Running aiohttp benchmark...")
            aiohttp_times = await self.benchmark_aiohttp(body_size)
            self.results["aiohttp"][body_size] = aiohttp_times

            # Print summary for this body size
            pyreqwest_avg = statistics.mean(pyreqwest_times)
            aiohttp_avg = statistics.mean(aiohttp_times)
            print(f"  pyreqwest average: {pyreqwest_avg:.4f}s")
            print(f"  aiohttp average: {aiohttp_avg:.4f}s")
            speedup = aiohttp_avg / pyreqwest_avg if pyreqwest_avg != 0 else 0
            print(f"  Speedup: {speedup:.2f}x")
            print()

    def create_box_plot(self) -> None:
        """Create box plot showing performance comparison with separate subplots per body size."""
        # Create subplots - 2 rows, 3 columns (with the last subplot empty for 5 body sizes)
        fig, axes = plt.subplots(2, 3, figsize=(15, 10))
        fig.suptitle("HTTP Client Performance Comparison: pyreqwest vs aiohttp", fontsize=16, y=0.98)

        # Flatten axes for easier iteration
        axes = axes.flatten()

        for i, body_size in enumerate(self.body_sizes):
            ax = axes[i]
            size_label = f"{body_size//1000}KB" if body_size < 1_000_000 else f"{body_size//1_000_000}MB"

            # Prepare data for this body size
            data_to_plot = [
                self.results["pyreqwest"][body_size],
                self.results["aiohttp"][body_size]
            ]
            labels = ["pyreqwest", "aiohttp"]

            # Create box plot for this body size
            box_plot = ax.boxplot(
                data_to_plot,
                patch_artist=True,
                tick_labels=labels,
                widths=0.6,
            )

            # Color the boxes
            colors = ["lightblue", "lightcoral"]
            for patch, color in zip(box_plot["boxes"], colors):
                patch.set_facecolor(color)

            # Customize subplot
            ax.set_title(f"{size_label} Body Size", fontweight='bold')
            ax.set_ylabel("Response Time (seconds)")
            ax.grid(True, alpha=0.3)

            # Add performance comparison text
            pyreqwest_avg = statistics.mean(self.results["pyreqwest"][body_size])
            aiohttp_avg = statistics.mean(self.results["aiohttp"][body_size])
            speedup = aiohttp_avg / pyreqwest_avg if pyreqwest_avg != 0 else 0

            # Add speedup annotation
            if speedup > 1:
                faster_lib = "pyreqwest"
                speedup_text = f"{speedup:.2f}x faster"
            else:
                faster_lib = "aiohttp"
                speedup_text = f"{1/speedup:.2f}x faster"

            ax.text(0.5, 0.95, f"{faster_lib} {speedup_text}",
                    transform=ax.transAxes, ha='center', va='top',
                    bbox=dict(boxstyle="round,pad=0.3", facecolor="wheat", alpha=0.8))

        # Remove the last empty subplot (we have 5 body sizes, 6 subplot positions)
        axes[-1].remove()

        # Add overall legend
        blue_patch = plt.Rectangle((0, 0), 1, 1, facecolor="lightblue", label="pyreqwest")
        coral_patch = plt.Rectangle((0, 0), 1, 1, facecolor="lightcoral", label="aiohttp")
        fig.legend(handles=[blue_patch, coral_patch], loc='lower right', bbox_to_anchor=(0.95, 0.02))

        plt.tight_layout()
        plt.subplots_adjust(top=0.93, bottom=0.08)  # Make room for suptitle and legend

        # Save the plot
        plt.savefig("performance_benchmark_boxplot.png", dpi=300, bbox_inches="tight")
        print("Box plot saved as 'performance_benchmark_boxplot.png'")

    def print_summary(self) -> None:
        """Print benchmark summary."""
        print("\n" + "="*60)
        print("PERFORMANCE BENCHMARK SUMMARY")
        print("="*60)

        for body_size in self.body_sizes:
            size_label = f"{body_size//1000}KB" if body_size < 1_000_000 else f"{body_size//1_000_000}MB"

            pyreqwest_times = self.results["pyreqwest"][body_size]
            aiohttp_times = self.results["aiohttp"][body_size]

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

            print(f"\n{size_label} Body Size:")
            print(f"  pyreqwest - Mean: {pyreqwest_stats['mean']:.4f}s, "
                  f"Median: {pyreqwest_stats['median']:.4f}s, "
                  f"StdDev: {pyreqwest_stats['stdev']:.4f}s")
            print(f"  aiohttp   - Mean: {aiohttp_stats['mean']:.4f}s, "
                  f"Median: {aiohttp_stats['median']:.4f}s, "
                  f"StdDev: {aiohttp_stats['stdev']:.4f}s")

            speedup = aiohttp_stats['mean'] / pyreqwest_stats['mean'] if pyreqwest_stats['mean'] != 0 else 0
            print(f"  Speedup: {speedup:.2f}x faster")


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
        benchmark.create_box_plot()
        benchmark.print_summary()


if __name__ == "__main__":
    asyncio.run(main())
