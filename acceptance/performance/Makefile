.PHONY: test build clean

# Run the performance acceptance test
test:
	@chmod +x run.sh
	@./run.sh

# Build both binaries
build:
	@echo "Building main http-playback-proxy binary..."
	@cd ../.. && cargo build --release
	@echo "Building performance test binary..."
	@cargo build --release

# Clean build artifacts
clean:
	@cargo clean
	@cd ../.. && cargo clean

# Quick test (assumes binaries are already built)
quick-test:
	@./target/release/performance-test || cargo run --release

help:
	@echo "Available targets:"
	@echo "  make test        - Run the complete performance acceptance test (builds + runs)"
	@echo "  make build       - Build all required binaries"
	@echo "  make quick-test  - Run test without rebuilding (faster, assumes binaries exist)"
	@echo "  make clean       - Clean build artifacts"
	@echo "  make help        - Show this help message"
