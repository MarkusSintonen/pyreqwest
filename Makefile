PYTHON_DIRS := python tests

.PHONY: install
install:
	uv sync

.PHONY: build
build:
	uv sync
	uv run maturin develop

.PHONY: test
test:
	uv run pytest

.PHONY: lint
lint:
	uv run ruff check $(PYTHON_DIRS)
	uv run ruff format --check $(PYTHON_DIRS)
	cargo fmt --check
	cargo clippy -- -D warnings

.PHONY: format
format:
	uv run ruff format $(PYTHON_DIRS)
	uv run ruff check --fix $(PYTHON_DIRS)
	cargo fmt
	cargo clippy --fix

.PHONY: type-check
type-check:
	uv run mypy $(PYTHON_DIRS)

.PHONY: static-checks
static-checks: lint type-check

.PHONY: check
check: static-checks test

.PHONY: clean
clean:
	rm -rf target/
	rm -f python/pyreqwest/_pyreqwest.cpython*
