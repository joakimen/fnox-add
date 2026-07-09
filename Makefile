BINARY := fnox-add

.DEFAULT_GOAL := build

# Build a release binary after checking formatting, lints, and tests.
.PHONY: build
build: fmt-check lint test
	cargo build --release

.PHONY: test
test:
	cargo test

# Lint; -D warnings fails the build on any warning.
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

# Install the release binary to ~/.cargo/bin.
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
