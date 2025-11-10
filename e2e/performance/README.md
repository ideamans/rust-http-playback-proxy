# Performance Acceptance Test

This is a comprehensive acceptance test that validates the timing accuracy of the HTTP Playback Proxy for both recording and playback modes.

## Overview

This test verifies that the proxy can:
1. **Record** HTTP traffic with accurate timing measurements (TTFB, transfer duration, bandwidth)
2. **Replay** recorded traffic with timing fidelity within ±10% of the original

The test simulates a realistic web scenario with:
- Multiple resources of varying sizes (10KB, 100KB, 1MB)
- Different latencies (500ms - 2000ms TTFB)
- Various transfer speeds
- Concurrent requests (up to 6 parallel connections, simulating browser behavior)

## Test Architecture

```
┌─────────────────────┐
│   Mock HTTPS Server │  ← Controlled latency & bandwidth
│   (self-signed TLS) │
└──────────┬──────────┘
           │
           ↓
┌─────────────────────┐
│  Recording Proxy    │  ← Records timing & content
│  (Phase 1)          │
└──────────┬──────────┘
           │
           ↓ Saves to
┌─────────────────────┐
│  index.json +   │  ← Verification (±10%)
│  resource files     │
└──────────┬──────────┘
           │
           ↓ Loaded by
┌─────────────────────┐
│  Playback Proxy     │  ← Replays with timing
│  (Phase 2)          │
└──────────┬──────────┘
           │
           ↓ Measured timing
┌─────────────────────┐
│  Timing Validation  │  ← Verification (±10%)
└─────────────────────┘
```

## Test Scenarios

### Test Resources

| Resource | Size | TTFB | Transfer Duration | Total Time |
|----------|------|------|-------------------|------------|
| /small   | 10KB | 500ms | 100ms | ~600ms |
| /medium  | 100KB | 1000ms | 500ms | ~1500ms |
| /large   | 1MB | 2000ms | 2000ms | ~4000ms |

### Verification Points

**Phase 1: Recording Validation**
- Verify `index.json` contains accurate timing data
- TTFB within ±10% of expected value
- Download end time within ±10% of expected value
- Bandwidth (Mbps) calculation accuracy

**Phase 2: Playback Validation**
- Replay TTFB within ±10% of recorded value
- Total transfer time within ±10% of recorded value
- Handle concurrent requests correctly
- Maintain timing accuracy under load

## Running the Test

### Prerequisites

1. Rust toolchain installed
2. Main `http-playback-proxy` binary built in release mode

### Quick Start

```bash
# Run the complete test (builds + executes)
make test

# Or use the shell script directly
./run.sh
```

### Manual Execution

```bash
# 1. Build main proxy binary (from repository root)
cd ../..
cargo build --release

# 2. Build test binary
cd accept/performance
cargo build --release

# 3. Run test
./target/release/performance-test
```

### Make Targets

```bash
make test        # Run complete test (recommended)
make build       # Build all binaries
make quick-test  # Run without rebuilding (faster)
make clean       # Clean build artifacts
make help        # Show help
```

## Test Duration

Expect the test to take approximately **10-15 seconds** to complete:
- Mock server startup: ~2s
- Recording phase: ~3-5s (parallel requests)
- Inventory verification: <1s
- Playback phase: ~3-5s (parallel requests)
- Cleanup: ~1s

## Success Criteria

The test passes if:

1. ✅ Mock HTTPS server starts successfully
2. ✅ Recording proxy captures all requests
3. ✅ Recorded TTFB is within ±10% of expected values
4. ✅ Recorded download times are within ±10% of expected values
5. ✅ Playback proxy replays with timing accuracy within ±10%
6. ✅ Concurrent requests are handled correctly
7. ✅ All resources are properly saved and retrieved

## Interpreting Results

### Successful Output

```
=== HTTP Playback Proxy Performance Acceptance Test ===

Step 1: Building main http-playback-proxy binary...
Main binary built successfully

Step 2: Building performance test binary...
Test binary built successfully

Step 3: Running performance acceptance test...

[INFO] Starting performance acceptance test
[INFO] Mock HTTPS server listening on https://127.0.0.1:18443
[INFO] Recording proxy started on port 18080
[INFO] All recording requests completed successfully
[INFO] Inventory verification passed
[INFO] Playback proxy started on port 18081
[INFO] All playback requests completed successfully

=== Performance Acceptance Test PASSED ===

========================================
  PERFORMANCE TEST PASSED!
========================================
```

### Failure Indicators

If the test fails, you'll see error messages like:

```
[ERROR] TTFB timing outside tolerance: measured=550ms, expected=500ms, diff=10.1%
[ERROR] Resource /medium not found in inventory
[ERROR] Playback request 3 failed: connection refused
```

## Troubleshooting

### Port Already in Use

The test uses ports 18080, 18081, and 18443. If you see "address already in use" errors:

```bash
# Find and kill processes using these ports
lsof -i :18080
lsof -i :18081
lsof -i :18443
kill -9 <PID>
```

### Timing Tolerance Failures

If timing is slightly outside the ±10% tolerance:
- This could indicate system load issues
- Try running the test on a less busy system
- Check for background processes affecting performance

### Binary Not Found

```
Binary not found at ../../target/release/http-playback-proxy
```

Solution:
```bash
cd ../..
cargo build --release
```

## Implementation Details

### Mock HTTPS Server

- Uses `rustls` for self-signed TLS certificates
- Generates certificates on-the-fly using `rcgen`
- Simulates realistic latency with `tokio::time::sleep`
- Controls bandwidth by chunked transfer with delays

### Timing Measurement

- Uses `std::time::Instant` for high-precision timing
- TTFB measured when response headers are received
- Total time measured when full body is downloaded
- Measurements use `reqwest` with proxy configuration

### Parallel Requests

- Simulates browser behavior with up to 6 concurrent connections
- Uses `futures::join_all` for parallel execution
- Each resource is requested twice to test caching behavior

## Contributing

When modifying this test:

1. Maintain the ±10% tolerance unless there's a strong reason to change it
2. Keep test duration under 20 seconds for developer experience
3. Add clear logging for debugging failures
4. Update this README if you add new test scenarios

## See Also

- [Main README](../../README.md) - Project overview
- [CLAUDE.md](../../CLAUDE.md) - Development guidelines
- [TypeScript Acceptance Tests](../typescript/) - Alternative language binding tests
- [Go Acceptance Tests](../golang/) - Go binding tests
