# E2E Tests

This directory contains end-to-end tests for the http-playback-proxy core functionality.

## Purpose

These tests ensure that the http-playback-proxy binary works correctly in production-like environments. They verify:

1. **Recording**: HTTP traffic can be recorded through the proxy
2. **Inventory**: Recorded data is saved and can be loaded correctly
3. **Playback**: Recorded traffic can be played back with accurate timing
4. **Performance**: Timing accuracy meets minimum requirements (performance/minimum tests)
5. **Content Processing**: Minified HTML/CSS/JS are beautified for editability (content test)

## Structure

```
e2e/
├── performance/     # Performance E2E tests (Rust)
│   ├── src/         # Test implementation
│   └── Makefile     # Build and test targets
├── minimum/         # Minimum timing E2E tests (Rust)
│   ├── src/         # Test implementation with various timing scenarios
│   └── run.sh       # Test runner script
├── content/         # Content beautification tests (Rust)
│   ├── src/         # Test minified HTML/CSS/JS beautification
│   └── run.sh       # Test runner script
└── Makefile         # Main test orchestration
```

## Running Tests Locally

### All Tests

```bash
cd e2e
make test-all
```

### Individual Tests

**Performance Test:**
```bash
cd e2e
make test-performance
```

**Minimum Timing Test:**
```bash
cd e2e
make test-minimum
```

**Content Beautification Test:**
```bash
cd e2e
make test-content
```

## Test Descriptions

### Performance Test (`performance/`)
- **Purpose**: End-to-end performance and stress testing
- **Verifies**: Recording, playback, content processing at scale
- **Runtime**: ~30-60 seconds

### Minimum Timing Test (`minimum/`)
- **Purpose**: Verify timing accuracy across different file sizes and latencies
- **Scenarios**:
  - 500KB files: fast (100ms TTFB, 200ms transfer), medium (500ms/1000ms), slow (1000ms/2000ms)
  - 1KB files: fast (100ms/100ms), medium (500ms/200ms), slow (1000ms/400ms)
- **Tolerance**: 10% deviation allowed
- **Verifies**: TTFB and transfer duration match expected values during recording and playback
- **Runtime**: ~60-90 seconds (6 scenarios × ~15 seconds each)

### Content Beautification Test (`content/`)
- **Purpose**: Verify minified HTML/CSS/JS are beautified during recording
- **Verifies**:
  - Minified HTML is beautified (line count increases 2x+)
  - Minified CSS is beautified (line count increases 2x+)
  - Minified JavaScript is beautified (line count increases 2x+)
  - `inventory.json` has `minify: true` flag for these resources
- **Why**: Makes recorded content editable for PageSpeed optimization testing
- **Runtime**: ~10 seconds

## How It Works

### Test Execution Flow

1. **Setup**: Build http-playback-proxy binary and test binaries
2. **Recording**:
   - Start recording proxy
   - Make HTTP requests through proxy
   - Stop proxy (saves inventory)
3. **Validation**:
   - Load inventory file
   - Verify all resources are present
   - Check content files exist
4. **Playback**:
   - Start playback proxy
   - Make same HTTP requests
   - Verify responses match recorded data
5. **Cleanup**: Stop proxy and clean up

### Key Features

- **Isolated**: Each test uses temporary directories
- **Realistic**: Uses actual HTTP traffic patterns
- **Comprehensive**: Tests full workflow from recording to playback
- **Content Quality**: Verifies minified resources are beautified for editability
- **Timing Accuracy**: Ensures playback timing matches recording within tolerance

## CI/CD Integration

These E2E tests can be integrated into CI pipelines. See the main [acceptance/](../acceptance/) directory for language wrapper tests that are automatically run by GitHub Actions.

## Troubleshooting

### "Binary not found" error

Make sure the binary exists:
```bash
cargo build --release
ls -la target/release/http-playback-proxy
```

### Tests timeout

- Increase timeout in test code
- Check system resources and network connectivity
- Verify binary is executable (permissions on Unix)

### Port conflicts

Tests use auto-assigned ports (port 0) to avoid conflicts. If you still see port errors:
- Wait a moment and retry
- Check for stuck processes: `lsof -i :8080`

## Adding New Tests

When adding new E2E tests:

1. Create a new directory under `e2e/`
2. Add test implementation with `src/main.rs`, `Cargo.toml`, and `run.sh` or `Makefile`
3. Update `e2e/Makefile` with new test target
4. Update this README.md with test description
5. Ensure tests are idempotent and use temporary directories
6. Test locally before committing (`make test-all`)

## See Also

- [Acceptance Tests README](../acceptance/README.md) - Language wrapper acceptance tests
- [Main README](../README.md) - Project documentation
