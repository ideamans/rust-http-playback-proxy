.PHONY: help build test-all test-performance test-minimum test-content test-contents test-typescript test-golang clean

# Default target
help:
	@echo "HTTP Playback Proxy - Acceptance Tests"
	@echo ""
	@echo "Available targets:"
	@echo "  make build            - Build main http-playback-proxy binary"
	@echo "  make test-all         - Run all acceptance tests (with build)"
	@echo "  make test-performance - Run performance acceptance test"
	@echo "  make test-minimum     - Run minimum timing acceptance test"
	@echo "  make test-content(s)  - Run content beautification acceptance test"
	@echo "  make test-typescript  - Run TypeScript acceptance test (with build)"
	@echo "  make test-golang      - Run Golang acceptance test (with build)"
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
	@echo "Copying binary to language bindings ($(PLATFORM))..."
	@mkdir -p ../typescript/bin/$(PLATFORM) ../golang/bin/$(PLATFORM)
	@rm -f ../typescript/bin/$(PLATFORM)/http-playback-proxy$(BINARY_EXT) ../golang/bin/$(PLATFORM)/http-playback-proxy$(BINARY_EXT)
	@cp -f ../target/release/http-playback-proxy$(BINARY_EXT) ../typescript/bin/$(PLATFORM)/http-playback-proxy$(BINARY_EXT)
	@cp -f ../target/release/http-playback-proxy$(BINARY_EXT) ../golang/bin/$(PLATFORM)/http-playback-proxy$(BINARY_EXT)
	@chmod +x ../typescript/bin/$(PLATFORM)/http-playback-proxy$(BINARY_EXT) ../golang/bin/$(PLATFORM)/http-playback-proxy$(BINARY_EXT)
	@echo "Binaries copied successfully"
	@echo ""

# Run all acceptance tests
test-all: build test-performance test-minimum test-content test-typescript test-golang
	@echo ""
	@echo "=========================================="
	@echo "  ALL ACCEPTANCE TESTS PASSED!"
	@echo "=========================================="
	@echo ""

# Performance acceptance test
test-performance:
	@echo ""
	@echo "=== Running Performance Acceptance Test ==="
	@echo ""
	@cd performance && $(MAKE) test

# Minimum timing acceptance test
test-minimum:
	@echo ""
	@echo "=== Running Minimum Timing Acceptance Test ==="
	@echo ""
	@cd minimum && ./run.sh

# Content beautification acceptance test
test-content:
	@echo ""
	@echo "=== Running Content Beautification Acceptance Test ==="
	@echo ""
	@cd content && ./run.sh

# Alias for test-content (plural form)
test-contents: test-content

# TypeScript acceptance test (requires latest binary)
test-typescript: build
	@echo ""
	@echo "=== Running TypeScript Acceptance Test ==="
	@echo ""
	@cd typescript && npm install && npm test

# Golang acceptance test (requires latest binary)
test-golang: build
	@echo ""
	@echo "=== Running Golang Acceptance Test ==="
	@echo ""
	@cd golang && go test -v -timeout 5m

# Clean all build artifacts
clean:
	@echo "Cleaning acceptance test artifacts..."
	@cd performance && $(MAKE) clean 2>/dev/null || true
	@cd minimum && cargo clean 2>/dev/null || true
	@cd content && cargo clean 2>/dev/null || true
	@cd typescript && rm -rf node_modules package-lock.json 2>/dev/null || true
	@cd golang && go clean 2>/dev/null || true
	@echo "Clean complete"
