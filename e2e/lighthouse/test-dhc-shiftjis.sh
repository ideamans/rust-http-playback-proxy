#!/bin/bash
set -e

# DHC Shift_JIS page test with Lighthouse
# Tests both recording and playback with actual Lighthouse runs

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROXY_BIN="${SCRIPT_DIR}/../../target/release/http-playback-proxy"
INVENTORY_DIR="/tmp/lighthouse-dhc-test"
TARGET_URL="https://top.dhc.co.jp/shop/ad/bofutsushosan/dd9qbtpu/index.html"
RECORDING_PORT=18080
PLAYBACK_PORT=18081

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "========================================"
echo "Lighthouse DHC Shift_JIS Test"
echo "========================================"
echo ""

# Check if binary exists
if [ ! -f "$PROXY_BIN" ]; then
    echo -e "${RED}Error: Binary not found at $PROXY_BIN${NC}"
    echo "Please run: cargo build --release"
    exit 1
fi

# Check if lighthouse is installed
if ! command -v lighthouse &> /dev/null; then
    echo -e "${RED}Error: lighthouse command not found${NC}"
    echo "Please install: npm install -g lighthouse"
    exit 1
fi

# Cleanup function
cleanup() {
    echo ""
    echo "Cleaning up..."
    if [ -n "$RECORDING_PID" ]; then
        kill -TERM "$RECORDING_PID" 2>/dev/null || true
        wait "$RECORDING_PID" 2>/dev/null || true
    fi
    if [ -n "$PLAYBACK_PID" ]; then
        kill -TERM "$PLAYBACK_PID" 2>/dev/null || true
        wait "$PLAYBACK_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

# Step 1: Recording with Lighthouse
echo "Step 1: Recording Phase"
echo "----------------------"
rm -rf "$INVENTORY_DIR"
mkdir -p "$INVENTORY_DIR"

echo "Starting recording proxy on port $RECORDING_PORT..."
"$PROXY_BIN" recording "$TARGET_URL" \
    --port "$RECORDING_PORT" \
    --inventory "$INVENTORY_DIR" \
    --device mobile \
    > "$INVENTORY_DIR/recording.log" 2>&1 &
RECORDING_PID=$!

echo "Recording proxy PID: $RECORDING_PID"
sleep 3

# Check if proxy started successfully
if ! kill -0 "$RECORDING_PID" 2>/dev/null; then
    echo -e "${RED}Error: Recording proxy failed to start${NC}"
    cat "$INVENTORY_DIR/recording.log"
    exit 1
fi

echo "Running Lighthouse through recording proxy..."
lighthouse "$TARGET_URL" \
    --chrome-flags="--ignore-certificate-errors --proxy-server=http://localhost:$RECORDING_PORT" \
    --throttling.rttMs=0 \
    --throttling.throughputKbps=0 \
    --throttling.requestLatencyMs=0 \
    --throttling.downloadThroughputKbps=0 \
    --throttling.uploadThroughputKbps=0 \
    --throttling.cpuSlowdownMultiplier=1 \
    --output=json \
    --output-path="$INVENTORY_DIR/recording-lighthouse.json" \
    --quiet \
    || true  # Lighthouse may fail due to proxy issues, but we continue

echo "Stopping recording proxy..."
kill -TERM "$RECORDING_PID"
wait "$RECORDING_PID" 2>/dev/null || true
RECORDING_PID=""

sleep 2

# Verify inventory was created
if [ ! -f "$INVENTORY_DIR/index.json" ]; then
    echo -e "${RED}Error: index.json not created${NC}"
    echo "Recording log:"
    cat "$INVENTORY_DIR/recording.log"
    exit 1
fi

echo -e "${GREEN}✓ Recording completed${NC}"
echo ""

# Check recorded resource
RESOURCE_COUNT=$(jq '.resources | length' "$INVENTORY_DIR/index.json")
echo "Recorded resources: $RESOURCE_COUNT"

if [ "$RESOURCE_COUNT" -eq 0 ]; then
    echo -e "${RED}Error: No resources recorded${NC}"
    exit 1
fi

# Check content charset
CONTENT_CHARSET=$(jq -r '.resources[0].contentCharset // "null"' "$INVENTORY_DIR/index.json")
echo "Content charset: $CONTENT_CHARSET"

if [ "$CONTENT_CHARSET" != "shift_jis" ] && [ "$CONTENT_CHARSET" != "Shift_JIS" ]; then
    echo -e "${YELLOW}Warning: Expected Shift_JIS charset, got: $CONTENT_CHARSET${NC}"
fi

# Check if content file exists
CONTENT_FILE=$(jq -r '.resources[0].contentFilePath // "null"' "$INVENTORY_DIR/index.json")
if [ "$CONTENT_FILE" != "null" ]; then
    FULL_CONTENT_PATH="$INVENTORY_DIR/$CONTENT_FILE"
    if [ -f "$FULL_CONTENT_PATH" ]; then
        FILE_ENCODING=$(file -b --mime-encoding "$FULL_CONTENT_PATH")
        FILE_SIZE=$(wc -c < "$FULL_CONTENT_PATH")
        echo "Content file: $CONTENT_FILE"
        echo "File encoding: $FILE_ENCODING"
        echo "File size: $FILE_SIZE bytes"

        # Check for charset declaration
        if grep -q 'charset="Shift_JIS"' "$FULL_CONTENT_PATH" || \
           grep -q "charset='Shift_JIS'" "$FULL_CONTENT_PATH" || \
           grep -q 'charset=Shift_JIS' "$FULL_CONTENT_PATH"; then
            echo -e "${GREEN}✓ Charset declaration preserved in file${NC}"
        else
            echo -e "${YELLOW}Warning: Shift_JIS charset declaration not found in file${NC}"
        fi
    fi
fi

echo ""

# Step 2: Playback with Lighthouse
echo "Step 2: Playback Phase"
echo "---------------------"
echo "Starting playback proxy on port $PLAYBACK_PORT..."
"$PROXY_BIN" playback \
    --port "$PLAYBACK_PORT" \
    --inventory "$INVENTORY_DIR" \
    > "$INVENTORY_DIR/playback.log" 2>&1 &
PLAYBACK_PID=$!

echo "Playback proxy PID: $PLAYBACK_PID"
sleep 3

# Check if proxy started successfully
if ! kill -0 "$PLAYBACK_PID" 2>/dev/null; then
    echo -e "${RED}Error: Playback proxy failed to start${NC}"
    cat "$INVENTORY_DIR/playback.log"
    exit 1
fi

echo "Running Lighthouse through playback proxy..."
lighthouse "$TARGET_URL" \
    --chrome-flags="--ignore-certificate-errors --proxy-server=http://localhost:$PLAYBACK_PORT" \
    --throttling.rttMs=0 \
    --throttling.throughputKbps=0 \
    --throttling.requestLatencyMs=0 \
    --throttling.downloadThroughputKbps=0 \
    --throttling.uploadThroughputKbps=0 \
    --throttling.cpuSlowdownMultiplier=1 \
    --output=json \
    --output-path="$INVENTORY_DIR/playback-lighthouse.json" \
    --quiet \
    || LIGHTHOUSE_EXIT_CODE=$?

if [ -n "$LIGHTHOUSE_EXIT_CODE" ] && [ "$LIGHTHOUSE_EXIT_CODE" -ne 0 ]; then
    echo -e "${RED}Error: Lighthouse playback failed with exit code $LIGHTHOUSE_EXIT_CODE${NC}"
    echo "Playback log:"
    tail -100 "$INVENTORY_DIR/playback.log"
    exit 1
fi

echo "Stopping playback proxy..."
kill -TERM "$PLAYBACK_PID"
wait "$PLAYBACK_PID" 2>/dev/null || true
PLAYBACK_PID=""

echo -e "${GREEN}✓ Playback completed${NC}"
echo ""

# Summary
echo "========================================"
echo "Test Summary"
echo "========================================"
echo "Inventory directory: $INVENTORY_DIR"
echo "Resources recorded: $RESOURCE_COUNT"
echo "Content charset: $CONTENT_CHARSET"
if [ -f "$INVENTORY_DIR/recording-lighthouse.json" ]; then
    echo "Recording Lighthouse report: $INVENTORY_DIR/recording-lighthouse.json"
fi
if [ -f "$INVENTORY_DIR/playback-lighthouse.json" ]; then
    echo "Playback Lighthouse report: $INVENTORY_DIR/playback-lighthouse.json"
fi
echo ""
echo -e "${GREEN}✓ All tests passed!${NC}"
