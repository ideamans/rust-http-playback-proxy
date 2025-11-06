# Acceptance Tests

This directory contains acceptance tests for the http-playback-proxy language wrappers.

## Purpose

These tests ensure that the Go and TypeScript wrappers work correctly in production-like environments before releasing to users. They verify:

1. **Binary Distribution**: Binaries are correctly bundled and located
2. **Recording**: HTTP traffic can be recorded through the proxy
3. **Inventory**: Recorded data is saved and can be loaded correctly
4. **Playback**: Recorded traffic can be played back with accurate timing

## Structure

```
accept/
├── golang/          # Go acceptance tests
│   ├── go.mod       # Go module with local dependency
│   ├── main_test.go # Test implementation
│   └── README.md    # Go-specific documentation
└── typescript/      # TypeScript acceptance tests
    ├── package.json # npm package with local dependency
    ├── test.js      # Test implementation
    └── README.md    # TypeScript-specific documentation
```

## Running Tests Locally

### Go

```bash
cd golang
go test -v -timeout 5m
```

### TypeScript

```bash
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

1. Add test cases to `main_test.go` (Go) or `test.js` (TypeScript)
2. Update README.md with test description
3. Ensure tests are idempotent and use temporary directories
4. Test locally before committing
5. Verify tests pass in CI

## See Also

- [Go Acceptance Test README](golang/README.md)
- [TypeScript Acceptance Test README](typescript/README.md)
- [GitHub Actions Workflow](../.github/workflows/acceptance-test.yml)
