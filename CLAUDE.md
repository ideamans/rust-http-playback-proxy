# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

HTTP Playback Proxy is a Rust-based MITM proxy for recording and replaying HTTP traffic with precise timing control. It's designed for PageSpeed optimization and performance testing by capturing web page loading behavior and replaying it with accurate timing simulation.

## Development Commands

```bash
# Build
cargo build --release

# Run tests (unit tests)
cargo test

# Run specific module tests
cargo test recording
cargo test playback

# Integration tests (requires binary build first)
cargo test --test integration_test --release -- --nocapture

# Lint and format
cargo clippy
cargo fmt

# Test coverage
cargo install cargo-tarpaulin  # First time only
cargo tarpaulin --out Html --output-dir coverage
```

## Pre-Commit Checklist

**IMPORTANT**: Before committing any changes, always run the following commands in order:

```bash
# 1. Check and fix formatting (same as CI)
cargo fmt --all -- --check
# If formatting issues found, fix them:
cargo fmt --all

# 2. Run strict linter checks (same as CI with -D warnings)
cargo clippy --all-targets --all-features -- -D warnings
# If warnings found, fix them automatically where possible:
cargo clippy --fix --all-targets --all-features --allow-dirty
# Then re-run the check to ensure all warnings are resolved

# 3. Run all tests (ensure all tests pass)
cargo test

# Only commit if all three steps complete successfully
```

**Why these specific commands:**
- `--all` ensures all workspace members are checked
- `-- --check` verifies formatting without modifying files (same as CI)
- `-D warnings` treats all warnings as errors (same strict level as CI)
- `--all-targets --all-features` checks production code, tests, and examples

This ensures code quality matches CI requirements and prevents broken commits from entering the repository.

## CLI Interface

```bash
# Recording mode
./target/release/http-playback-proxy recording [entry_url] \
  --port <port> \
  --device <desktop|mobile> \
  --inventory <inventory_dir>

# Playback mode
./target/release/http-playback-proxy playback \
  --port <port> \
  --inventory <inventory_dir>

# Defaults:
# - Port: auto-search from 8080
# - Device: mobile
# - Inventory: ./inventory
```

## Architecture

### Module Structure

```
src/
├── main.rs                    # Entry point
├── cli.rs                     # Clap command definitions
├── types.rs                   # Core data types (Resource, Inventory, Transaction, BodyChunk)
├── traits.rs                  # DI traits (FileSystem, TimeProvider, HttpClient)
├── utils.rs                   # Utilities (port search, encode/decode, minify)
├── recording/
│   ├── mod.rs                 # Recording mode entry point
│   ├── proxy.rs               # Recording HTTP proxy server
│   ├── hudsucker_handler.rs   # MITM request/response handler
│   └── processor.rs           # Response processing (compression, charset, minify)
└── playback/
    ├── mod.rs                 # Playback mode entry point
    ├── proxy.rs               # Playback HTTP proxy server
    └── transaction.rs         # Resource to Transaction conversion
```

### Key Design Patterns

**Trait-Based Dependency Injection**

Framework-free trait-based DI for testability:

- `FileSystem` - File I/O abstraction
- `TimeProvider` - Time measurement abstraction
- `HttpClient` - HTTP request abstraction (for future extensions)
- Mock implementations in `traits::mocks` module

**Recording Flow**
1. Start HTTP proxy server (listen on specified port)
2. Capture requests and forward to upstream servers
3. Stream responses to clients while recording in memory:
   - TTFB (Time To First Byte)
   - Headers, status code, body (compressed)
   - Transfer time (for Mbps calculation)
4. On Ctrl+C (SIGINT), save inventory and process resources:
   - Decompress response bodies
   - Convert text resources to UTF-8
   - Beautify and detect minification (2x+ line increase = minified)
   - Save to `inventory_dir/contents/<method>/<protocol>/<path>`

**Playback Flow**
1. Load `inventory.json`
2. Convert Resources to Transactions:
   - Re-minify if `minify: true`
   - Re-encode (gzip/br/etc)
   - Split into chunks with timestamps
3. Start HTTP proxy server
4. Match requests to Transactions
5. Replay with timing control:
   - Wait until TTFB
   - Send chunks according to `targetTime` (simulating original transfer speed)

### Data Type Compatibility

**Important**: `Resource` and `Inventory` types must maintain strong compatibility with TypeScript definitions in `reference/types.ts`. Other internal types (Transaction, BodyChunk) can be optimized for performance.

### Content File Path Generation

URL to file path conversion rules:

- Base: `inventory_dir/contents/<method>/<protocol>/<path>`
- Index handling: `/` → `/index.html`
- Query parameters:
  - ≤32 chars: `resource~param=value.html`
  - >32 chars: `resource~param=first32chars.~<sha1(rest)>.html`

### Performance Considerations

- **Memory-first design**: Assumes typical web page sizes, uses memory generously for speed
- **Async/await**: Tokio runtime for concurrent request handling
- **Streaming**: Efficient handling of large responses
- **Pre-processing**: Playback mode pre-converts all Resources to Transactions for fast delivery

## Testing Strategy

### Unit Tests

- Located in `<module>/tests.rs` files
- Use mock implementations from `traits::mocks`
- Focus on isolated component behavior

### Integration Tests

Implemented in `tests/integration_test.rs`. Full end-to-end test:

1. Build binary (`cargo build` or `cargo build --release`)
2. Start embedded static HTTP server (serving HTML/CSS/JS)
3. Start recording proxy with temporary inventory directory
4. Make HTTP requests through proxy using `reqwest` client
5. Send SIGINT to recording proxy (graceful shutdown)
6. Verify `inventory.json` and `contents/` files
7. Start playback proxy with recorded data
8. Verify playback responses match recordings (whitespace-normalized)

**Note**: Integration tests use dynamic port discovery to avoid conflicts. On Unix systems, uses `libc::kill()` for graceful SIGINT shutdown.

### Acceptance Tests

Located in `acceptance/` directory. Three main test suites:

1. **Performance Test** (`acceptance/performance/`):
   - End-to-end performance and stress testing
   - Verifies recording, playback, and content processing at scale
   - Runtime: ~30-60 seconds

2. **Minimum Timing Test** (`acceptance/minimum/`):
   - Tests timing accuracy across different file sizes and latencies
   - 6 scenarios testing various TTFB/transfer duration combinations
   - Verifies 10% tolerance for timing accuracy
   - Runtime: ~60-90 seconds

3. **Content Beautification Test** (`acceptance/content/`):
   - Verifies minified HTML/CSS/JS are beautified during recording
   - Ensures `minify: true` flag is set correctly
   - Makes recorded content editable for PageSpeed optimization
   - Runtime: ~10 seconds

Run all acceptance tests:
```bash
cd acceptance
make test-all
```

## Technology Stack

- **Rust Edition**: 2024
- **Async Runtime**: Tokio 1.0 (all features)
- **HTTP Stack**: Hyper 1.0, Hyper-util, Http-body-util, Tower/Tower-http
- **MITM Proxy**: Hudsucker 0.24 (with rcgen-ca for self-signed certificates)
- **Serialization**: Serde + serde_json
- **Compression**: flate2 (gzip/deflate), brotli
- **Encoding**: encoding_rs (charset conversion)
- **Minify**: prettyish-html, prettify-js (beautification)
- **CLI**: Clap 4.5 (derive features)
- **Testing**: tempfile, tokio-test, reqwest (for integration tests)

## Common Patterns

### Adding New Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::mocks::{MockFileSystem, MockTimeProvider};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_something() {
        let fs = Arc::new(MockFileSystem::new());
        let time = Arc::new(MockTimeProvider::new(0));

        // Test implementation
    }
}
```

### Working with Inventory

```rust
use crate::types::{Inventory, Resource};

let mut inventory = Inventory::new();
inventory.entry_url = Some("https://example.com".to_string());
inventory.device_type = Some(DeviceType::Desktop);

let resource = Resource::new("GET".to_string(), "https://example.com".to_string());
inventory.resources.push(resource);
```

## Troubleshooting

**Integration tests fail with "port already in use"**
- Uses dynamic port discovery, but manual cleanup may be needed:
  ```bash
  lsof -i :8080
  kill -9 <PID>
  ```

**SIGINT not working in integration tests**
- Unix-only feature (uses `libc::kill`)
- Windows fallback uses force termination

**Binary not found during tests**
- Run `cargo build` or `cargo build --release` before integration tests
- Tests check both debug and release binary locations

## Implementation Details

### Recording Mode Processing

- First request received is time origin (0 seconds)
- Response body recorded as-is if compressed
- Record response body length and time from response start to completion
- Mbps calculation: `(response body length / (response end - response start) seconds) / (1024 * 1024)`

### Text Resource Processing

Special handling for key text resources (HTML, CSS, JavaScript):

1. Convert to UTF-8:
   - Reference Charset from headers or content declaration
   - After UTF-8 conversion, update header Charset to UTF-8 and remove content Charset declarations

2. Beautify processing:
   - If beautified line count is 2x+ original, resource was minified
   - Set resource `minify: true` flag

### Playback Mode Pre-processing

- After loading Inventory, convert all Resources to Transactions
- Transaction includes:
  - Pre-processed information for returning HTTP responses
  - Re-minify if `minify: true`
  - Encode and split into chunks
  - Target send time for each chunk (including TTFB offset)

### HTTPS Support

- Operates as MITM proxy
- Uses self-signed certificates (rcgen-ca)
- Standard TLS certificate validation (public websites work out-of-box)
- For self-signed certificates, add to system trust store

## Recent Fixes (2025-11-08)

### ✅ Request/Response Correlation (Phase 3 - Critical)
**Issue**: Global FIFO queue caused request/response mismatches with concurrent connections
**Fix**: Changed to per-connection FIFO queues (`HashMap<SocketAddr, VecDeque<RequestInfo>>`)
- Each connection has independent queue
- HTTP/1.1 pipelining supported
- Prevents cross-connection response mix-ups
- All tests (76 unit + 1 integration) passing

### ✅ HTTP Multi-Value Headers (Phase 1)
**Issue**: `HashMap<String, String>` lost Set-Cookie and other multi-value headers
**Fix**: Changed to `HashMap<String, HeaderValue>` with Single/Multiple enum
- Recording: Auto-detect duplicate headers and convert to Multiple
- Playback: `as_vec()` correctly restores all values
- TypeScript compatibility maintained (serde untagged)

### ✅ Host-Based Transaction Matching (Phase 4)
**Issue**: Matching only by method + path + query, ignoring host
**Fix**: Added host (authority) to matching logic
- Extract host from Host header or URI authority
- Require host match when both have host information
- Backward compatible (path-only matching when host info missing)

### ✅ Final Chunk Timing Control (Phase 4)
**Issue**: Last chunk waited until `target_close_time` before sending
**Fix**: All chunks sent at `target_time`, connection closed after waiting for `target_close_time`
- All chunks (including last): Send according to `target_time`
- After all chunks sent: Wait until `target_close_time` before closing connection
- Accurately reproduces send completion timing

## Language Wrappers

### Go Module

Located in `golang/`. Wraps Rust binary as Go process manager with Inventory read/write helpers.

```go
package main

import (
    "fmt"
    proxy "github.com/pagespeed-quest/http-playback-proxy/golang"
)

func main() {
    p, err := proxy.StartRecording(proxy.RecordingOptions{
        EntryURL:     "https://example.com",
        Port:         8080,
        DeviceType:   proxy.DeviceTypeMobile,
        InventoryDir: "./inventory",
    })
    if err != nil {
        panic(err)
    }

    // ... make requests ...

    p.Stop()
}
```

See [golang/README.md](golang/README.md) for details.

### TypeScript/Node.js Module

Located in `typescript/`. Similar wrapper for Node.js environments.

```typescript
import { startRecording } from 'http-playback-proxy';

async function record() {
  const proxy = await startRecording({
    entryUrl: 'https://example.com',
    port: 8080,
    deviceType: 'mobile',
    inventoryDir: './inventory',
  });

  // ... make requests ...

  await proxy.stop();
}
```

See [typescript/README.md](typescript/README.md) for details.

## Release Workflow

Multi-platform build and release process using GitHub Actions:

1. **Create version tag**: `git tag v0.0.0 && git push origin v0.0.0`
2. **GitHub Actions builds** binaries for all platforms:
   - darwin-arm64 (macOS Apple Silicon)
   - darwin-amd64 (macOS Intel)
   - linux-amd64 (Linux x86_64)
   - linux-arm64 (Linux ARM64)
   - windows-amd64 (Windows x86_64)
3. **update-binaries workflow** automatically:
   - Downloads platform binaries
   - Places in `golang/bin/` and `typescript/bin/`
   - Updates TypeScript `package.json` version
   - Creates pull request
4. **acceptance-test workflow** runs on PR:
   - Tests all platforms with binaries
   - Verifies recording, playback, and timing accuracy
5. **After PR merge**:
   - Tag Go module: `git tag golang/v0.0.0 && git push origin golang/v0.0.0`
   - Publish to npm: `cd typescript && npm publish`

See [README.md](README.md#release-workflow) for detailed workflow.
