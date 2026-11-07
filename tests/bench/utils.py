import os
import platform
import statistics
from collections import defaultdict
from datetime import UTC, datetime
from importlib.metadata import version
from pathlib import Path
from typing import Self

from pydantic import BaseModel, Field

RESULTS_DIR = Path(__file__).parent / "results"


class RunEnv(BaseModel, frozen=True):
    version: str  # Package version
    python: str
    os: str

    @classmethod
    def current(cls, lib: str) -> Self:
        return cls(
            version=package_version(lib),
            python=f"{platform.python_implementation()} {platform.python_version()}",
            os=platform.platform(terse=True),
        )


class Stats(BaseModel):
    lib: str
    body_size: int
    concurrency: int
    timings: list[float]
    env: RunEnv | None = None

    @property
    def case(self) -> tuple[str, int, int]:
        return self.lib, self.body_size, self.concurrency

    @property
    def median(self) -> float:
        return statistics.median(self.timings) if self.timings else 0


class StatsCollection(BaseModel):
    stats: list[Stats] = Field(default_factory=list)

    def find(self, lib: str, body_size: int, concurrency: int) -> Stats:
        for stat in self.stats:
            if stat.case == (lib, body_size, concurrency):
                return stat
        raise RuntimeError(f"Missing stats for {lib}, size={body_size}, concurrency={concurrency}")

    @classmethod
    def load(cls) -> Self:
        runs_by_case: defaultdict[tuple[str, int, int], list[Stats]] = defaultdict(list)
        for file in sorted(RESULTS_DIR.glob("*.json")):
            for stats in cls.model_validate_json(file.read_text()).stats:
                runs_by_case[stats.case].append(stats)

        best = []
        for runs in runs_by_case.values():
            latest_env = runs[-1].env
            candidates = [run for run in runs if run.env == latest_env]
            # Cases that could not be benchmarked have no timings, so prefer any run that has them
            best.append(min(candidates, key=lambda run: (not run.timings, run.median)))
        return cls(stats=best)

    def save_run(self, lib: str) -> Path:
        run_id = os.environ.get("BENCH_RUN_ID") or datetime.now(UTC).strftime("%Y%m%dT%H%M%SZ")
        RESULTS_DIR.mkdir(exist_ok=True)
        path = RESULTS_DIR / f"{run_id}_{lib}.json"
        tmp_path = path.with_suffix(".tmp")
        tmp_path.write_text(self.model_dump_json())
        tmp_path.replace(path)  # Atomic, so an interrupted write never leaves a broken file
        return path


def package_version(lib: str) -> str:
    return version("pyreqwest" if lib.startswith("pyreqwest") else lib.replace("_", "-"))


def fmt_size(size: int) -> str:
    return f"{size // 1000}KB" if size < 1_000_000 else f"{size // 1_000_000}MB"


def is_sync(lib: str) -> bool:
    return lib == "urllib3" or lib.startswith("pyreqwest_sync")
