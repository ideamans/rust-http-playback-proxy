#!/bin/bash

set -e

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== HTTP Playback Proxy Performance Acceptance Test ===${NC}"
echo ""

# Step 1: Build the main binary
echo -e "${YELLOW}Step 1: Building main http-playback-proxy binary...${NC}"
cd ../..
# Remove existing binary to ensure fresh build
rm -f target/release/http-playback-proxy
cargo build --release
if [ $? -ne 0 ]; then
    echo -e "${RED}Failed to build main binary${NC}"
    exit 1
fi
echo -e "${GREEN}Main binary built successfully${NC}"
echo ""

# Step 2: Build the test binary
echo -e "${YELLOW}Step 2: Building performance test binary...${NC}"
cd accept/performance
# Remove existing test binary to ensure fresh build
rm -f target/release/performance-test
cargo build --release
if [ $? -ne 0 ]; then
    echo -e "${RED}Failed to build test binary${NC}"
    exit 1
fi
echo -e "${GREEN}Test binary built successfully${NC}"
echo ""

# Step 3: Run the test
echo -e "${YELLOW}Step 3: Running performance acceptance test...${NC}"
echo -e "${YELLOW}This test will take approximately 10-15 seconds to complete${NC}"
echo ""

./target/release/performance-test

if [ $? -eq 0 ]; then
    echo ""
    echo -e "${GREEN}========================================${NC}"
    echo -e "${GREEN}  PERFORMANCE TEST PASSED!${NC}"
    echo -e "${GREEN}========================================${NC}"
    exit 0
else
    echo ""
    echo -e "${RED}========================================${NC}"
    echo -e "${RED}  PERFORMANCE TEST FAILED!${NC}"
    echo -e "${RED}========================================${NC}"
    exit 1
fi
