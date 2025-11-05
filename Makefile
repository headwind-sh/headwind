# Headwind Makefile
# Common development tasks

.PHONY: help build test fmt lint check clean install run docker pre-commit all

# Default target
help:
	@echo "Headwind Development Commands:"
	@echo ""
	@echo "  make build       - Build the project"
	@echo "  make test        - Run tests"
	@echo "  make fmt         - Format code"
	@echo "  make lint        - Run clippy lints"
	@echo "  make check       - Check compilation without building"
	@echo "  make clean       - Clean build artifacts"
	@echo "  make install     - Install development tools"
	@echo "  make run         - Run the project locally"
	@echo "  make docker      - Build Docker image"
	@echo "  make pre-commit  - Run pre-commit hooks"
	@echo "  make all         - Run fmt, lint, and test"
	@echo ""

# Build the project
build:
	@echo "Building project..."
	cargo build

# Build release
build-release:
	@echo "Building release..."
	cargo build --release

# Run tests
test:
	@echo "Running tests..."
	cargo test --all-features

# Run tests with output
test-verbose:
	@echo "Running tests (verbose)..."
	cargo test --all-features -- --nocapture

# Format code
fmt:
	@echo "Formatting code..."
	cargo fmt --all

# Check formatting
fmt-check:
	@echo "Checking code formatting..."
	cargo fmt --all -- --check

# Run clippy
lint:
	@echo "Running clippy..."
	cargo clippy --all-features --all-targets -- -D warnings

# Check compilation
check:
	@echo "Checking compilation..."
	cargo check --all-features --all-targets

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	cargo clean

# Install development tools
install:
	@echo "Installing development tools..."
	cargo install cargo-audit
	cargo install cargo-deny
	cargo install cargo-udeps
	cargo install cargo-tarpaulin
	cargo install cargo-watch
	pip install pre-commit
	pre-commit install

# Run the project
run:
	@echo "Running headwind..."
	RUST_LOG=headwind=debug cargo run

# Watch and run tests on file changes
watch:
	@echo "Watching for changes..."
	cargo watch -x test

# Build Docker image
docker:
	@echo "Building Docker image..."
	docker build -t headwind:dev .

# Run Docker container
docker-run:
	@echo "Running Docker container..."
	docker run --rm -it headwind:dev

# Load Docker image into kind
kind-load:
	@echo "Loading Docker image into kind..."
	docker build -t headwind:dev .
	kind load docker-image headwind:dev

# Security audit
audit:
	@echo "Running security audit..."
	cargo audit

# Check dependencies
deny:
	@echo "Checking dependencies..."
	cargo deny check

# Check for unused dependencies
udeps:
	@echo "Checking for unused dependencies..."
	cargo +nightly udeps --all-targets

# Generate documentation
docs:
	@echo "Generating documentation..."
	cargo doc --all-features --no-deps --open

# Run pre-commit hooks
pre-commit:
	@echo "Running pre-commit hooks..."
	pre-commit run --all-files

# Code coverage
coverage:
	@echo "Generating code coverage..."
	cargo tarpaulin --all-features --workspace --timeout 120 --out Html

# Run all quality checks
all: fmt lint test
	@echo "✓ All checks passed!"

# CI simulation (what runs in GitHub Actions)
ci: fmt-check lint test check audit deny
	@echo "✓ CI checks passed!"

# Quick check before commit
quick: fmt lint
	@echo "✓ Quick checks passed!"
