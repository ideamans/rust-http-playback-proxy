# Go Acceptance Test

This directory contains acceptance tests for the Go wrapper of http-playback-proxy.

## Purpose

These tests verify that the Go wrapper works correctly in a production-like environment:
- Recording HTTP traffic through the proxy
- Saving and loading inventory files
- Playing back recorded traffic with accurate timing

## Prerequisites

1. Build the Rust binary first:
   ```bash
   cd ../..
   cargo build --release
   ```

2. Copy the binary to the appropriate platform directory:
   ```bash
   # For your platform
   mkdir -p golang/bin/<platform>
   cp target/release/http-playback-proxy golang/bin/<platform>/
   ```

## Running the Tests

### Locally

```bash
# Download dependencies
go mod download

# Run tests
go test -v -timeout 5m
```

### Environment Variables

- `HTTP_PLAYBACK_PROXY_CACHE_DIR`: Override the cache directory for binaries
  - Useful for testing with local binaries instead of downloading from GitHub

### What the Tests Do

1. **Recording Test**:
   - Starts a test HTTP server
   - Starts a recording proxy
   - Makes HTTP requests through the proxy
   - Verifies inventory and content files are created

2. **Load Inventory Test**:
   - Loads the recorded inventory
   - Validates all resources have required fields
   - Checks that content files exist

3. **Playback Test**:
   - Starts a playback proxy with the recorded inventory
   - Makes HTTP requests through the proxy
   - Verifies responses match the recorded content

## CI/CD Integration

These tests are automatically run by GitHub Actions on pull requests that include binary updates.
See `.github/workflows/acceptance-test.yml` for details.

## Troubleshooting

**"Binary not found" error:**
- Make sure you've built the Rust binary and copied it to `golang/bin/<platform>/`
- Or set `HTTP_PLAYBACK_PROXY_CACHE_DIR` to point to where binaries are located

**Port already in use:**
- The tests use port 0 (auto-assign) by default, so this should be rare
- If it happens, wait a moment and try again

**Tests timeout:**
- Increase the timeout: `go test -v -timeout 10m`
