.PHONY: test
test:
	uv run maturin develop --uv
	uv run pytest

.PHONY: test-release
test-release:
	uv run maturin develop --uv --release
	uv run pytest

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
	uv run mypy .

.PHONY: static-checks
static-checks: lint type-check

.PHONY: check
check: static-checks test

.PHONY: bench
bench:
	uv run maturin develop --uv --release
	uv run python -m tests.bench.benchmark_performance --lib $(lib)

.PHONY: clean
clean:
	rm -rf target/
	rm -f python/pyreqwest/*.so
	rm -f *.profraw
	rm -rf coverage/

.PHONY: testcov
testcov:
	rm -f python/pyreqwest/*.so
	rm -f *.profraw
	rm -rf coverage/
	RUSTFLAGS='-C instrument-coverage' uv run maturin develop --uv
	uv run pytest
	grcov . \
		--binary-path ./python/pyreqwest/*.so \
		--source-dir ./src \
		--output-type html \
		--output-path ./coverage \
		--html-resources=cdn \
		--branch \
		--ignore-not-existing \
		--ignore '**/build.rs' \
		--excl-start ':NOCOV_START' \
		--excl-stop ':NOCOV_END' \
		--excl-line ':NOCOV|^( )+}$$|unreachable!|^#\['
	rm -f *.profraw

.PHONY: docs
docs:
	uv run maturin develop --uv
	uv run pdoc -o $(outdir) --no-show-source pyreqwest.client.types pyreqwest

.PHONY: docs-browser
docs-browser:
	uv run maturin develop --uv
	uv run pdoc --no-show-source pyreqwest.client.types pyreqwest
