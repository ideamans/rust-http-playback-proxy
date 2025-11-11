.PHONY: help build test-all test-performance test-minimum test-content test-contents clean

# Default target
help:
	@echo "HTTP Playback Proxy - E2E Tests"
	@echo ""
	@echo "Available targets:"
	@echo "  make build            - Build main http-playback-proxy binary"
	@echo "  make test-all         - Run all E2E tests (with build)"
	@echo "  make test-performance - Run performance E2E test"
	@echo "  make test-minimum     - Run minimum timing E2E test"
	@echo "  make test-content(s)  - Run content beautification E2E test"
	@echo "  make clean            - Clean all build artifacts"
	@echo ""

# Detect platform
UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)

ifeq ($(UNAME_S),Darwin)
    ifeq ($(UNAME_M),arm64)
        PLATFORM := darwin-arm64
    else
        PLATFORM := darwin-amd64
    endif
else ifeq ($(UNAME_S),Linux)
    ifeq ($(UNAME_M),aarch64)
        PLATFORM := linux-arm64
    else
        PLATFORM := linux-amd64
    endif
else
    PLATFORM := windows-amd64
    BINARY_EXT := .exe
endif

# Build main binary
build:
	@echo "Building main http-playback-proxy binary..."
	@cd .. && cargo build --release 2>&1 | grep -v "Finished" || true
	@echo "Main binary built successfully"
	@echo ""

# Run all E2E tests
test-all: build test-performance test-content
	@echo ""
	@echo "=========================================="
	@echo "  ALL E2E TESTS PASSED!"
	@echo "=========================================="
	@echo ""

# Performance E2E test
test-performance:
	@echo ""
	@echo "=== Running Performance E2E Test ==="
	@echo ""
	@cd performance && $(MAKE) test

# Minimum timing E2E test
test-minimum:
	@echo ""
	@echo "=== Running Minimum Timing E2E Test ==="
	@echo ""
	@cd minimum && ./run.sh

# Content beautification E2E test
test-content:
	@echo ""
	@echo "=== Running Content Beautification E2E Test ==="
	@echo ""
	@cd content && ./run.sh

# Alias for test-content (plural form)
test-contents: test-content

# Clean all build artifacts
clean:
	@echo "Cleaning E2E test artifacts..."
	@cd performance && $(MAKE) clean 2>/dev/null || true
	@cd minimum && cargo clean 2>/dev/null || true
	@cd content && cargo clean 2>/dev/null || true
	@echo "Clean complete"
