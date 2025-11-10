# Minimum Timing Acceptance Test

This is a minimal acceptance test that validates the timing accuracy of the HTTP Playback Proxy for recording and playback modes.

## Overview

This test verifies that the proxy can:
1. **Record** HTTP traffic with accurate timing measurements (TTFB, transfer duration)
2. **Replay** recorded traffic with timing fidelity within ±10% of the original

The test uses a single 500KB file with three different scenarios:
- **Fast**: TTFB 100ms, Transfer 200ms (~2.5MB/s)
- **Medium**: TTFB 500ms, Transfer 1000ms (~0.5MB/s)
- **Slow**: TTFB 1000ms, Transfer 2000ms (~0.25MB/s)

## Test Flow

For each scenario:

```
┌─────────────────────┐
│   Mock HTTP Server  │  ← Controlled latency & bandwidth
│   (500KB file)      │
└──────────┬──────────┘
           │
           ↓
┌─────────────────────┐
│  Recording Proxy    │  ← Records timing & content
└──────────┬──────────┘
           │
           ↓
┌─────────────────────┐
│  Inventory          │  ← Verify recorded timing (±10%)
│  (index.json)   │
└──────────┬──────────┘
           │
           ↓
┌─────────────────────┐
│  Playback Proxy     │  ← Replays with timing
└──────────┬──────────┘
           │
           ↓
┌─────────────────────┐
│  Timing Validation  │  ← Verify playback (±10%)
└─────────────────────┘
```

## Running the Test

### Quick Start

```bash
# Run the complete test (builds + executes)
./run.sh
```

### Manual Execution

```bash
# 1. Build main proxy binary (from repository root)
cd ../..
cargo build --release

# 2. Build test binary
cd accept/minimum
cargo build --release

# 3. Run test
./target/release/minimum-timing-test
```

## Test Duration

Expect the test to take approximately **30-40 seconds** to complete:
- 3 scenarios × (recording ~5s + playback ~5s + startup ~3s) ≈ 39s

## Success Criteria

The test passes if all three scenarios meet these criteria:

1. ✅ Mock HTTP server starts successfully
2. ✅ Recording proxy captures the request
3. ✅ Recorded TTFB is within ±10% of expected value
4. ✅ Recorded transfer time is within ±10% of expected value
5. ✅ Playback proxy replays with TTFB within ±10%
6. ✅ Playback total time is within ±10%

## Interpreting Results

### Successful Output

```
=== Minimum Timing Acceptance Test ===
Testing single 500KB file with various latencies and transfer speeds

=== Testing scenario: fast ===
...
Recording completed:
  TTFB: 102ms
  Total: 305ms
Inventory verification PASSED
Playback completed:
  TTFB: 104ms
  Total: 310ms
Playback timing verification PASSED
=== Scenario 'fast' PASSED ===

... (medium and slow scenarios) ...

=================================
  ALL TESTS PASSED!
=================================
```

### Failure Indicators

If timing is outside tolerance:

```
[ERROR] TTFB timing outside tolerance: measured=150ms, expected=100ms, diff=50.0%
```

This indicates the playback proxy is not accurately reproducing the recorded timing.

## Common Issues

### Port Already in Use

The test uses ports 17080, 17081, and 17082. If you see "address already in use" errors:

```bash
lsof -i :17080
lsof -i :17081
lsof -i :17082
kill -9 <PID>
```

### Timing Tolerance Failures

If timing is consistently outside the ±10% tolerance:
- Check system load (high CPU usage can affect timing)
- Verify the playback proxy's timing implementation
- This usually indicates a bug in the chunking/timing logic

## Purpose

This test was created to identify and fix issues in the playback proxy's timing reproduction. It serves as a baseline for ensuring that:

1. Recording accurately captures timing information
2. Playback accurately reproduces the recorded timing
3. The system works with files of practical size (500KB)
4. Various network conditions (fast/medium/slow) are handled correctly

## See Also

- [Main README](../../README.md) - Project overview
- [CLAUDE.md](../../CLAUDE.md) - Development guidelines
- [Performance Tests](../performance/) - More comprehensive timing tests
