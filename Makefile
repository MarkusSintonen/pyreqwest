.PHONY: test
test:
	uv run maturin develop --uv --all-features
	uv run pytest
	uv run pytest tests/pytest_mock/test_plugin_external.py

.PHONY: test-release
test-release:
	uv run maturin develop --uv --all-features --release
	uv run pytest
	uv run pytest tests/pytest_mock/test_plugin_external.py

.PHONY: lint
lint:
	uv run ruff check .
	uv run ruff format --check .
	cargo fmt --check
	cargo clippy -- -D warnings

.PHONY: format
format:
	uv run ruff format .
	uv run ruff check --fix .
	cargo fmt
	cargo clippy --fix --allow-dirty

.PHONY: type-check
type-check:
	uv sync --all-groups --no-group build
	uv run mypy .

.PHONY: static-checks
static-checks: lint type-check

.PHONY: check
check: static-checks test

.PHONY: bench
bench:
	scripts/bench.sh

.PHONY: clean
clean:
	rm -rf target/
	rm -f python/pyreqwest/*.so
	rm -f *.profraw
	rm -rf coverage/

.PHONY: testcov
testcov:
	scripts/testcov.sh

.PHONY: docs
docs:
	uv run maturin develop --uv --all-features
	uv run pdoc -o $(outdir) --no-show-source pyreqwest.client.types pyreqwest

.PHONY: docs-browser
docs-browser:
	uv run maturin develop --uv --all-features
	uv run pdoc --no-show-source pyreqwest.client.types pyreqwest

.PHONY: profile
profile:
	uv run maturin develop --uv --all-features --profile profiling
	samply record uv run python -m tests.bench.latency --lib pyreqwest

.PHONY: bump-deps
bump-deps:
	cargo upgrade --pinned
	cargo update
	uv run python ./scripts/bump_py_deps.py
