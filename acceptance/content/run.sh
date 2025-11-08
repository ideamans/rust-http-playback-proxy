#!/bin/bash
set -e

echo "=== Content Beautification Acceptance Test ==="
echo ""

# Build main proxy binary
echo "Step 1: Building main http-playback-proxy binary..."
cd ../..
cargo build --release
echo "Main binary built successfully"
echo ""

# Build test binary
echo "Step 2: Building test binary..."
cd acceptance/content
cargo build --release
echo "Test binary built successfully"
echo ""

# Run test
echo "Step 3: Running content beautification test..."
echo ""
./target/release/content-test

# Check exit code
if [ $? -eq 0 ]; then
    echo ""
    echo "========================================"
    echo "  CONTENT TEST PASSED!"
    echo "========================================"
    exit 0
else
    echo ""
    echo "========================================"
    echo "  CONTENT TEST FAILED!"
    echo "========================================"
    exit 1
fi
