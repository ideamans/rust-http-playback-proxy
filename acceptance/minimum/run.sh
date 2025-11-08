#!/bin/bash
set -e

echo "=== Minimum Timing Acceptance Test ==="
echo ""

# Build main proxy binary
echo "Step 1: Building main http-playback-proxy binary..."
cd ../..
cargo build --release
echo "Main binary built successfully"
echo ""

# Build test binary
echo "Step 2: Building test binary..."
cd acceptance/minimum
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
