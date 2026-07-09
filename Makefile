BINARY := fnox-add

.DEFAULT_GOAL := build

# Build a release binary after checking formatting, lints, and tests.
.PHONY: build
build: fmt-check lint test
	cargo build --release

.PHONY: test
test:
	cargo test

# Clippy is Rust's linter; -D warnings makes any warning fail the build.
.PHONY: lint
lint:
	cargo clippy --all-targets -- -D warnings

# Format the code in place.
.PHONY: fmt
fmt:
	cargo fmt

# Verify formatting without changing files (used by `make build` and CI).
.PHONY: fmt-check
fmt-check:
	cargo fmt --check

# Install to ~/.cargo/bin (already on your PATH).
.PHONY: install
install:
	cargo install --path .

# Run without installing, e.g. `make run ARGS="--dry-run --group personal"`.
.PHONY: run
run:
	cargo run -- $(ARGS)

.PHONY: clean
clean:
	cargo clean
