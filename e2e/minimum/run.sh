#!/bin/bash
set -e

echo "=== Minimum Timing Acceptance Test ==="
echo ""

# Get the script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# Build main proxy binary
echo "Step 1: Building main http-playback-proxy binary..."
cd "${PROJECT_ROOT}"
cargo build --release
echo "Main binary built successfully"
echo ""

# Build test binary
echo "Step 2: Building test binary..."
cd "${SCRIPT_DIR}"
cargo build --release
echo "Test binary built successfully"
echo ""

# Run test
echo "Step 3: Running minimum timing test..."
echo ""
./target/release/minimum-timing-test

# Check exit code
if [ $? -eq 0 ]; then
    echo ""
    echo "========================================"
    echo "  MINIMUM TIMING TEST PASSED!"
    echo "========================================"
    exit 0
else
    echo ""
    echo "========================================"
    echo "  MINIMUM TIMING TEST FAILED!"
    echo "========================================"
    exit 1
fi
