#!/bin/bash
set -e

echo "=== Minimum Timing Acceptance Test ==="
echo ""

# Get the script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# Check/build main proxy binary
echo "Step 1: Checking main http-playback-proxy binary..."
cd "${PROJECT_ROOT}"
if [ ! -f target/release/http-playback-proxy ] && [ ! -f target/release/http-playback-proxy.exe ]; then
    echo "Binary not found, building..."
    cargo build --release
    echo "Main binary built successfully"
else
    echo "Main binary already exists, skipping build"
fi
echo ""

# Check/build test binary
echo "Step 2: Checking test binary..."
cd "${SCRIPT_DIR}"
if [ ! -f target/release/minimum-timing-test ] && [ ! -f target/release/minimum-timing-test.exe ]; then
    echo "Test binary not found, building..."
    cargo build --release
    echo "Test binary built successfully"
else
    echo "Test binary already exists, skipping build"
fi
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
