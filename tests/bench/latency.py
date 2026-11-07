import argparse
import asyncio

from pyreqwest.http import Url

from tests.bench.gc import capture_gc_stats
from tests.bench.runner import Runner
from tests.bench.server import CaCert, server
from tests.bench.utils import RunEnv, Stats, StatsCollection, fmt_size, is_sync

FULL_CONSUME_SIZE_LIMIT = 1_000_000


class PerformanceLatency:
    def __init__(self, server_url: Url, lib: str, ca_cert: CaCert) -> None:
        self.lib = lib
        self.body_sizes = [
            2_000_000,  # 2MB (streamed)
            1_000_000,  # 1MB
            100_000,  # 100KB
            10_000,  # 10KB
        ]
        self.concurrency_levels = [20, 5, 2] if is_sync(lib) else [100, 10, 2]
        self.runner = Runner(
            url=server_url.with_query({"echo_only_body": "1"}),
            ca_cert=ca_cert,
            full_consume_size_limit=FULL_CONSUME_SIZE_LIMIT,
            stream_write_chunk_size=256 * 1024,
            stream_read_chunk_size=65536,
            num_requests=100,
            warmup_iterations=10,
            iterations=40,
        )

    async def run_benchmarks(self) -> None:
        print(f"Starting performance benchmark for {self.lib}...")
        print(f"Body sizes: {[fmt_size(size) for size in self.body_sizes]}")
        print(f"Concurrency levels: {self.concurrency_levels}")
        print(f"Warmup iterations: {self.runner.warmup_iterations}")
        print(f"Benchmark iterations: {self.runner.iterations}")
        print()

        bodies = [b"x" * size for size in self.body_sizes]
        results: list[tuple[int, int, list[float]]] = []

        with capture_gc_stats(self.lib):
            for body in bodies:
                for concurrency in self.concurrency_levels:
                    timings = await self.runner.run_lib(self.lib, body, concurrency)
                    results.append((len(body), concurrency, timings))
                    if timings:
                        print(f"{self.lib} average: {(sum(timings) / len(timings)):.4f}ms\n")
                    else:
                        print(f"{self.lib} N/A\n")

        env = RunEnv.current(self.lib)
        collection = StatsCollection(
            stats=[
                Stats(lib=self.lib, body_size=body_sz, concurrency=concurrency, timings=timings, env=env)
                for body_sz, concurrency, timings in results
            ]
        )
        print(f"Results saved to '{collection.save_run(self.lib)}'")


async def main() -> None:
    parser = argparse.ArgumentParser(description="Performance latency")
    parser.add_argument("--lib", type=str)
    args = parser.parse_args()

    async with server() as (url, ca_cert):
        await PerformanceLatency(url, args.lib, ca_cert).run_benchmarks()


if __name__ == "__main__":
    asyncio.run(main())
