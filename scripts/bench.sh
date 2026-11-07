#!/bin/bash
# Usage: [BENCH_REPEATS=n] scripts/bench.sh [lib ...]   (defaults to all libs, 1 repeat)
# Each lib writes tests/bench/results/<run_id>_<lib>.json. Per benchmark case, the latest run decides the environment
# (package version, Python, OS) and the best result among runs of that environment is used (see tests/bench/utils.py).
# Repeating runs all libs n times so every lib gets the same number of runs.
# To undo a (partial) run, delete its files: rm tests/bench/results/<run_id>_*
set -euo pipefail

all_libs=(
  "pyreqwest_st"
  "pyreqwest_mt"
  "pyreqwest_sync_st"
  "pyreqwest_sync_mt"
  "aiohttp"
  "urllib3"
  "httpx"
  "httpx2"
  "rnet"
  "niquests"
  "curl_cffi"
)
if [[ $# -gt 0 ]]; then
  libs=("$@")
else
  libs=("${all_libs[@]}")
fi
for lib in "${libs[@]}"; do
  if [[ ! " ${all_libs[*]} " == *" $lib "* ]]; then
    echo "Unknown lib: $lib (available: ${all_libs[*]})" >&2
    exit 1
  fi
done

uv sync --all-groups --no-group build
uv run maturin develop --uv --all-features --release

for ((repeat = 1; repeat <= ${BENCH_REPEATS:-1}; repeat++)); do
  export BENCH_RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)"
  echo "Bench run $repeat/${BENCH_REPEATS:-1} id: $BENCH_RUN_ID (undo with: rm tests/bench/results/${BENCH_RUN_ID}_*)"
  for lib in "${libs[@]}"; do
    uv run python -m tests.bench.latency --lib "$lib"
  done
done

for lib in "${libs[@]}"; do
  if [[ "$lib" != *"pyreqwest"* ]]; then
    uv run python -m tests.bench.plots --lib "$lib"
  fi
done
