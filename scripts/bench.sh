#!/bin/bash
set -euo pipefail

uv sync --all-groups --no-group build
uv run maturin develop --uv --all-features --release

libs=(
  "pyreqwest_st"
  "pyreqwest_mt"
  "pyreqwest_sync_st"
  "pyreqwest_sync_mt"
  "aiohttp"
  "urllib3"
  "httpx"
  "rnet"
  # "ry"
  "niquests"
  "curl_cffi"
)
for lib in "${libs[@]}"; do
  uv run python -m tests.bench.latency --lib "$lib"
done

for lib in "${libs[@]}"; do
  if [[ "$lib" != *"pyreqwest"* ]]; then
    uv run python -m tests.bench.plots --lib "$lib"
  fi
done
