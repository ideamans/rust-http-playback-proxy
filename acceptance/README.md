# Acceptance Tests

This directory contains acceptance tests for the http-playback-proxy language wrappers.

## Purpose

These tests ensure that the http-playback-proxy works correctly in production-like environments. They verify:

1. **Binary Distribution**: Binaries are correctly bundled and located (Go/TypeScript wrappers)
2. **Recording**: HTTP traffic can be recorded through the proxy
3. **Inventory**: Recorded data is saved and can be loaded correctly
4. **Playback**: Recorded traffic can be played back with accurate timing
5. **Performance**: Timing accuracy meets minimum requirements (performance/minimum tests)
6. **Content Processing**: Minified HTML/CSS/JS are beautified for editability (content test)

## Structure

```
acceptance/
├── performance/     # Performance acceptance tests (Rust)
│   ├── src/         # Test implementation
│   └── Makefile     # Build and test targets
├── minimum/         # Minimum timing acceptance tests (Rust)
│   ├── src/         # Test implementation with various timing scenarios
│   └── run.sh       # Test runner script
├── content/         # Content beautification tests (Rust)
│   ├── src/         # Test minified HTML/CSS/JS beautification
│   └── run.sh       # Test runner script
├── golang/          # Go acceptance tests
│   ├── go.mod       # Go module with local dependency
│   ├── main_test.go # Test implementation
│   └── README.md    # Go-specific documentation
├── typescript/      # TypeScript acceptance tests
│   ├── package.json # npm package with local dependency
│   ├── test.js      # Test implementation
│   └── README.md    # TypeScript-specific documentation
└── Makefile         # Main test orchestration
```

## Running Tests Locally

### All Tests

```bash
cd acceptance
make test-all
```

### Individual Tests

**Performance Test:**
```bash
cd acceptance
make test-performance
```

**Minimum Timing Test:**
```bash
cd acceptance
make test-minimum
```

**Content Beautification Test:**
```bash
cd acceptance
make test-content
```

**Go Wrapper Test:**
```bash
cd acceptance
make test-golang
# or directly:
cd golang
go test -v -timeout 5m
```

**TypeScript Wrapper Test:**
```bash
cd acceptance
make test-typescript
# or directly:
cd typescript
npm install
npm test
```

## CI/CD Integration

These tests are automatically run by GitHub Actions when:
1. A PR is created that modifies binaries (`golang/bin/**` or `typescript/bin/**`)
2. The acceptance test code itself is modified
3. Manually triggered via workflow_dispatch

The workflow runs tests on multiple platforms:
- Ubuntu (linux-amd64)
- macOS ARM64 (darwin-arm64)
- macOS Intel (darwin-amd64)
- Windows (windows-amd64)

## How It Works

### Test Execution Flow

1. **Setup**: Start a test HTTP server
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
- **Realistic**: Uses actual Go modules / npm packages
- **Comprehensive**: Tests full workflow from recording to playback
- **Cross-platform**: Runs on all supported platforms
- **Content Quality**: Verifies minified resources are beautified for editability
- **Timing Accuracy**: Ensures playback timing matches recording within tolerance

## Environment Variables

Both tests support the following environment variables:

- `HTTP_PLAYBACK_PROXY_CACHE_DIR`: Override binary cache directory
  - Useful for testing with specific binary locations
  - In CI, set to force use of PR binaries

## Troubleshooting

### "Binary not found" error

Make sure binaries exist in the expected locations:
- Go: `golang/bin/<platform>/http-playback-proxy[.exe]`
- TypeScript: `typescript/bin/<platform>/http-playback-proxy[.exe]`

In CI, this error means the PR doesn't include binaries for all platforms.

### Tests timeout

- Increase timeout: `go test -v -timeout 10m` or adjust Node.js test timeout
- Check system resources and network connectivity
- Verify binary is executable (permissions on Unix)

### Port conflicts

Tests use auto-assigned ports (port 0) to avoid conflicts. If you still see port errors:
- Wait a moment and retry
- Check for stuck processes: `lsof -i :8080`

## Release Workflow

The acceptance tests are a critical part of the release workflow:

```
1. Tag v0.0.0 → Trigger release.yml
2. Build binaries for all platforms
3. Create GitHub Release
4. Trigger update-binaries.yml
5. Download binaries to golang/bin/ and typescript/bin/
6. Create PR with binaries
7. Trigger acceptance-test.yml on PR ← You are here
8. Run tests on all platforms
9. If tests pass → PR can be merged
10. After merge → Tag golang/v0.0.0 and npm publish
```

This ensures that binaries are tested before being released to users.

## Adding New Tests

When adding new tests:

1. **Rust tests**: Create a new directory under `acceptance/` with `src/main.rs`, `Cargo.toml`, and `run.sh`
2. **Language wrapper tests**: Add test cases to `main_test.go` (Go) or `test.js` (TypeScript)
3. Update `acceptance/Makefile` with new test target
4. Update this README.md with test description
5. Ensure tests are idempotent and use temporary directories
6. Test locally before committing (`make test-all`)
7. Verify tests pass in CI

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

### Language Wrapper Tests (`golang/`, `typescript/`)
- **Purpose**: Verify binary distribution and wrapper API functionality
- **Verifies**: End-to-end workflow from language-specific package perspective

## See Also

- [Go Acceptance Test README](golang/README.md)
- [TypeScript Acceptance Test README](typescript/README.md)
- [GitHub Actions Workflow](../.github/workflows/acceptance-test.yml)
