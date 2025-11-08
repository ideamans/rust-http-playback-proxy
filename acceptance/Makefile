.PHONY: help test-all test-performance test-minimum test-typescript test-golang clean

# Default target
help:
	@echo "HTTP Playback Proxy - Acceptance Tests"
	@echo ""
	@echo "Available targets:"
	@echo "  make test-all         - Run all acceptance tests"
	@echo "  make test-performance - Run performance acceptance test"
	@echo "  make test-minimum     - Run minimum timing acceptance test"
	@echo "  make test-typescript  - Run TypeScript acceptance test"
	@echo "  make test-golang      - Run Golang acceptance test"
	@echo "  make clean            - Clean all build artifacts"
	@echo ""

# Run all acceptance tests
test-all: test-performance test-minimum test-typescript test-golang
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

# TypeScript acceptance test
test-typescript:
	@echo ""
	@echo "=== Running TypeScript Acceptance Test ==="
	@echo ""
	@cd typescript && npm install && npm test

# Golang acceptance test
test-golang:
	@echo ""
	@echo "=== Running Golang Acceptance Test ==="
	@echo ""
	@cd golang && go test -v -timeout 5m

# Clean all build artifacts
clean:
	@echo "Cleaning acceptance test artifacts..."
	@cd performance && $(MAKE) clean 2>/dev/null || true
	@cd minimum && cargo clean 2>/dev/null || true
	@cd typescript && rm -rf node_modules package-lock.json 2>/dev/null || true
	@cd golang && go clean 2>/dev/null || true
	@echo "Clean complete"
