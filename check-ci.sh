#!/bin/bash
set -e

echo "Running CI checks locally..."
echo ""

# Set same RUSTFLAGS as CI
export RUSTFLAGS="-D warnings"
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_TERM_COLOR=always
export RUST_BACKTRACE=short

echo "1. Checking formatting..."
cargo fmt --all -- --check
echo "✓ Formatting OK"
echo ""

echo "2. Running clippy..."
cargo clippy --all-targets --all-features -- -D warnings
echo "✓ Clippy OK"
echo ""

echo "3. Running tests..."
cargo test -- --test-threads=1
echo "✓ Tests OK"
echo ""

echo "All CI checks passed!"
