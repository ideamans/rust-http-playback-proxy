# HTTP Playback Proxy

[日本語](./README_ja.md) | English

[![CI](https://github.com/pagespeed-quest/http-playback-proxy/actions/workflows/ci.yml/badge.svg)](https://github.com/pagespeed-quest/http-playback-proxy/actions/workflows/ci.yml)
[![Release](https://github.com/pagespeed-quest/http-playback-proxy/actions/workflows/release.yml/badge.svg)](https://github.com/pagespeed-quest/http-playback-proxy/actions/workflows/release.yml)

An MITM HTTP/HTTPS proxy for recording and replaying web traffic with precise timing control. Designed for PageSpeed optimization, performance testing, and automated web performance analysis.

## Features

- **Recording Mode**: Capture HTTP/HTTPS traffic as MITM proxy with timing metadata
- **Playback Mode**: Replay recorded traffic with accurate TTFB and transfer duration simulation
- **Content Processing**: Automatic beautification of minified HTML/CSS/JS for editability
- **HTTPS Support**: Transparent HTTPS proxy using self-signed certificates
- **Timing Accuracy**: ±10% timing precision for TTFB and transfer duration
- **Multi-Platform**: Supports macOS (ARM64/x86_64), Linux (x86_64/ARM64), Windows (x86_64)
- **Language Wrappers**: Go and TypeScript/Node.js bindings for easy integration

## Quick Start

### Using Pre-built Binaries

Download from [GitHub Releases](https://github.com/pagespeed-quest/http-playback-proxy/releases):

```bash
# macOS ARM64
curl -L https://github.com/pagespeed-quest/http-playback-proxy/releases/latest/download/http-playback-proxy-darwin-arm64.tar.gz | tar xz

# Linux x86_64
curl -L https://github.com/pagespeed-quest/http-playback-proxy/releases/latest/download/http-playback-proxy-linux-amd64.tar.gz | tar xz

# Windows x86_64
# Download http-playback-proxy-windows-amd64.zip and extract
```

### Command Line Usage

#### Recording Mode

**Basic recording (auto-searches port from 18080):**
```bash
./http-playback-proxy recording https://example.com
```

**Full options:**
```bash
./http-playback-proxy recording https://example.com \
  --port 18080 \             # Proxy port (default: 18080, auto-search if occupied)
  --device mobile \           # Device type: mobile or desktop (default: mobile)
  --inventory ./my-session    # Output directory (default: ./inventory)
```

**Recording workflow:**
1. Start proxy: `./http-playback-proxy recording https://example.com`
2. Configure browser proxy to `127.0.0.1:18080` (or displayed port)
3. Visit website in browser
4. Press `Ctrl+C` (or send SIGTERM/SIGINT) to stop and save recording
5. Check `./inventory/index.json` and `./inventory/contents/`

**Manual browsing (no entry URL):**
```bash
# Start proxy and browse manually
./http-playback-proxy recording --port 18080
```

#### Playback Mode

**Basic playback:**
```bash
./http-playback-proxy playback --inventory ./my-session
```

**Full options:**
```bash
./http-playback-proxy playback \
  --port 18080 \              # Proxy port (default: 18080, auto-search if occupied)
  --inventory ./my-session    # Recorded data directory (default: ./inventory)
```

**Playback workflow:**
1. Start proxy: `./http-playback-proxy playback --inventory ./my-session`
2. Configure browser proxy to `127.0.0.1:18080` (or displayed port)
3. Visit same website - responses match recorded timing (±10%)
4. Press `Ctrl+C` (or send SIGTERM/SIGINT) to stop

#### Browser Proxy Configuration

**Chrome/Chromium:**
```bash
# macOS/Linux
google-chrome --proxy-server="127.0.0.1:8080"

# Windows
chrome.exe --proxy-server="127.0.0.1:8080"
```

**Firefox:**
Settings → Network Settings → Manual proxy configuration:
- HTTP Proxy: `127.0.0.1`, Port: `8080`
- Check "Also use this proxy for HTTPS"

**System-wide (macOS):**
```bash
# Set proxy
networksetup -setwebproxy Wi-Fi 127.0.0.1 8080
networksetup -setsecurewebproxy Wi-Fi 127.0.0.1 8080

# Unset proxy
networksetup -setwebproxystate Wi-Fi off
networksetup -setsecurewebproxystate Wi-Fi off
```

## Installation

### From Source (Rust)

```bash
git clone https://github.com/pagespeed-quest/http-playback-proxy.git
cd http-playback-proxy
cargo build --release
```

Binary location: `target/release/http-playback-proxy`

### Go Module

```bash
go get github.com/pagespeed-quest/http-playback-proxy/golang
```

**Recording example:**
```go
package main

import (
    "fmt"
    "time"
    proxy "github.com/pagespeed-quest/http-playback-proxy/golang"
)

func main() {
    // Start recording proxy
    p, err := proxy.StartRecording(proxy.RecordingOptions{
        EntryURL:     "https://example.com",
        Port:         8080,
        DeviceType:   proxy.DeviceTypeMobile,
        InventoryDir: "./inventory",
    })
    if err != nil {
        panic(err)
    }

    fmt.Printf("Recording proxy started on port %d\n", p.Port)

    // Wait or do HTTP requests through the proxy
    time.Sleep(30 * time.Second)

    // Stop and save recording
    if err := p.Stop(); err != nil {
        panic(err)
    }

    // Load and analyze inventory
    inventory, err := p.GetInventory()
    if err != nil {
        panic(err)
    }

    fmt.Printf("Recorded %d resources\n", len(inventory.Resources))
}
```

**Playback example:**
```go
// Start playback proxy
p, err := proxy.StartPlayback(proxy.PlaybackOptions{
    Port:         8080,
    InventoryDir: "./inventory",
})
if err != nil {
    panic(err)
}

// Wait for requests
time.Sleep(30 * time.Second)

// Stop playback
p.Stop()
```

**Working with inventory:**
```go
// Load inventory from index.json
inventory, err := proxy.LoadInventory("./inventory/index.json")
if err != nil {
    panic(err)
}

// Iterate resources
for i, resource := range inventory.Resources {
    fmt.Printf("%d: %s %s (TTFB: %dms)\n",
        i, resource.Method, resource.URL, resource.TtfbMs)

    // Get content file path
    if resource.ContentFilePath != nil {
        contentPath := proxy.GetResourceContentPath("./inventory", &resource)
        // Read content file...
    }
}
```

See [golang/README.md](golang/README.md) for full API documentation.

### TypeScript/Node.js Package

```bash
npm install http-playback-proxy
```

**Recording example:**
```typescript
import { startRecording } from 'http-playback-proxy';

async function record() {
  // Start recording proxy
  const proxy = await startRecording({
    entryUrl: 'https://example.com',
    port: 8080,
    deviceType: 'mobile',
    inventoryDir: './inventory',
  });

  console.log(`Recording proxy started on port ${proxy.port}`);

  // Wait or do HTTP requests through the proxy
  await new Promise(resolve => setTimeout(resolve, 30000));

  // Stop and save recording
  await proxy.stop();

  // Load and analyze inventory
  const inventory = await proxy.getInventory();
  console.log(`Recorded ${inventory.resources.length} resources`);
}

record().catch(console.error);
```

**Playback example:**
```typescript
import { startPlayback } from 'http-playback-proxy';

async function playback() {
  // Start playback proxy
  const proxy = await startPlayback({
    port: 8080,
    inventoryDir: './inventory',
  });

  console.log(`Playback proxy started on port ${proxy.port}`);

  // Wait for requests
  await new Promise(resolve => setTimeout(resolve, 30000));

  // Stop playback
  await proxy.stop();
}

playback().catch(console.error);
```

**Working with inventory:**
```typescript
import { loadInventory, getResourceContentPath } from 'http-playback-proxy';

// Load inventory
const inventory = await loadInventory('./inventory/index.json');

// Iterate resources
for (const [i, resource] of inventory.resources.entries()) {
  console.log(`${i}: ${resource.method} ${resource.url} (TTFB: ${resource.ttfbMs}ms)`);

  // Get content file path
  if (resource.contentFilePath) {
    const contentPath = getResourceContentPath('./inventory', resource);
    // Read content file...
  }
}
```

See [typescript/README.md](typescript/README.md) for full API documentation.

## Architecture

### Core Implementation (Rust)

- **Runtime**: Tokio async runtime for concurrent request handling
- **HTTP Stack**: Hyper 1.0, Hyper-util, Tower/Tower-http
- **MITM Proxy**: Hudsucker 0.24 with rcgen-ca for self-signed certificates
- **Content Processing**: Automatic beautification (prettyish-html, prettify-js)
- **Compression**: gzip, deflate, brotli support (flate2, brotli crates)
- **Encoding**: Charset detection and UTF-8 conversion (encoding_rs)

### Data Structure

Recordings are stored as:
- `index.json`: Metadata for all resources (URLs, timing, headers)
- `contents/`: Content files organized by method/protocol/path

**Inventory Structure:**
```json
{
  "entryUrl": "https://example.com",
  "deviceType": "mobile",
  "resources": [
    {
      "method": "GET",
      "url": "https://example.com/style.css",
      "ttfbMs": 150,
      "mbps": 2.5,
      "statusCode": 200,
      "rawHeaders": {
        "content-type": "text/css; charset=utf-8"
      },
      "contentEncoding": "gzip",
      "contentFilePath": "get/https/example.com/style.css",
      "minify": true
    }
  ]
}
```

### Language Wrappers

**Go**: Process manager wrapper with Inventory helpers
- Manages binary lifecycle (start/stop)
- Type-safe Inventory reading/writing
- Goroutine-based request handling

**TypeScript/Node.js**: Similar wrapper for Node.js ecosystem
- Promise-based API
- npm package distribution
- Full TypeScript type definitions

## Testing Ecosystem

### Unit Tests (Rust)

```bash
cargo test                    # All unit tests
cargo test recording          # Recording module tests
cargo test playback           # Playback module tests
cargo test -- --nocapture     # With detailed output
```

### Integration Tests (Rust)

Located in `tests/integration_test.rs`. Full end-to-end Rust test:

```bash
cargo test --test integration_test --release -- --nocapture
```

Tests: Recording → Inventory saving → Playback → Content verification

### E2E Tests (Core Functionality)

Located in `e2e/`. Tests core binary functionality:

```bash
cd e2e
make test-all                 # All E2E tests
make test-performance         # Performance/stress test
make test-minimum             # Timing accuracy test (6 scenarios)
make test-content             # Content beautification test
```

**Minimum Timing Test** (`e2e/minimum/`):
- Tests 6 scenarios with different file sizes and latencies
- Verifies ±10% timing accuracy for TTFB and transfer duration
- Runtime: ~60-90 seconds

**Content Beautification Test** (`e2e/content/`):
- Verifies minified HTML/CSS/JS are beautified during recording
- Checks `minify: true` flag in inventory
- Ensures content is editable for PageSpeed optimization
- Runtime: ~10 seconds

See [e2e/README.md](e2e/README.md) for details.

### Acceptance Tests (Language Wrappers)

Located in `acceptance/`. Tests Go and TypeScript wrappers:

```bash
cd acceptance
make test-all                 # Test both Go and TypeScript wrappers
make test-golang              # Go wrapper only
make test-typescript          # TypeScript wrapper only
```

**Go Acceptance Test** (`acceptance/golang/`):
- Verifies Go API (StartRecording, StartPlayback, Stop)
- Tests binary distribution in Go module
- Validates Inventory reading/writing

**TypeScript Acceptance Test** (`acceptance/typescript/`):
- Verifies TypeScript API
- Tests binary distribution in npm package
- Validates Promise-based workflow

See [acceptance/README.md](acceptance/README.md) for details.

## CI/CD Workflow

Pre-commit checks (run locally):
```bash
./check-ci.sh                 # Runs exact CI checks locally
```

This script runs:
1. `cargo fmt --all -- --check` - Formatting verification
2. `cargo clippy --all-targets --all-features -- -D warnings` - Strict linting
3. `cargo test` - All tests (unit + integration)

### Release Workflow

Multi-platform automated release:

```
1. Create tag:          git tag v0.0.0 && git push origin v0.0.0
2. GitHub Actions:      Build binaries for 5 platforms (release.yml)
3. Create Release:      Publish to GitHub Releases
4. Auto-trigger:        update-binaries.yml workflow
5. Create PR:           Binaries → golang/bin/ and typescript/bin/
6. Run Acceptance:      Test all platforms (acceptance-test.yml)
7. Merge PR:            After tests pass
8. Tag Go module:       git tag golang/v0.0.0 && git push
9. Publish npm:         cd typescript && npm publish
```

Supported platforms:
- darwin-arm64 (macOS Apple Silicon)
- darwin-amd64 (macOS Intel)
- linux-amd64 (Linux x86_64)
- linux-arm64 (Linux ARM64)
- windows-amd64 (Windows x86_64)

## Development

### Project Structure

```
.
├── src/                     # Rust core implementation
│   ├── recording/           # Recording mode (MITM proxy, response processing)
│   ├── playback/            # Playback mode (timing control, transaction matching)
│   └── ...
├── tests/                   # Rust integration tests
├── e2e/                     # Core E2E tests (performance, timing, content)
├── acceptance/              # Language wrapper acceptance tests
├── golang/                  # Go language wrapper + tests
├── typescript/              # TypeScript/Node.js wrapper + tests
└── .github/workflows/       # CI/CD workflows
```

### Code Quality

**Pre-commit checks (recommended):**
```bash
./check-ci.sh                # Runs exact CI checks locally
```

This script runs:
1. `cargo fmt --all -- --check` - Formatting verification
2. `cargo clippy --all-targets --all-features -- -D warnings` - Strict linting
3. `cargo test` - All tests (unit + integration)

**Individual commands:**
```bash
cargo fmt                    # Auto-format code
cargo clippy                 # Lint check
cargo test                   # Run all tests
cargo build --release        # Release build
```

### Key Implementation Features

**Recording:**
- MITM proxy using Hudsucker with self-signed certificates
- Per-connection FIFO queues for request/response correlation
- Automatic content beautification (minified HTML/CSS/JS)
- Multi-value header support (e.g., Set-Cookie)
- UTF-8 conversion and charset detection

**Playback:**
- Precise timing control (±10% accuracy for TTFB and transfer duration)
- Chunk-based response streaming with target times
- Transaction matching by method + host + path + query
- Automatic re-minification and re-encoding

**Testability:**
- Trait-based dependency injection (FileSystem, TimeProvider)
- Mock implementations for unit testing
- Comprehensive test coverage (unit, integration, E2E, acceptance)

## Troubleshooting

**Proxy Connection Issues:**
- Verify port availability: `lsof -i :8080`
- Check firewall settings
- Verify browser proxy configuration

**HTTPS Certificate Errors:**
- Browser: Click "Advanced" → "Proceed" (certificate is self-signed)
- System trust: Add certificate to system trust store if needed

**Binary Not Found (Tests):**
```bash
cargo build --release
ls -la target/release/http-playback-proxy
```

**Port Conflicts:**
- Tests use auto-assigned ports (port 0)
- Kill stuck processes: `lsof -i :8080 && kill -9 <PID>`

**Timing Inaccuracy:**
- Check system load (high CPU/disk usage affects timing)
- Verify network stability
- See minimum timing test for expected tolerances

## Contributing

Contributions are welcome! Please:
1. Run `./check-ci.sh` before committing
2. Add tests for new features
3. Update documentation
4. Follow existing code style

## License

[Add license information]

## See Also

- [CLAUDE.md](CLAUDE.md) - Development guidance for AI assistants
- [E2E Tests README](e2e/README.md) - Core E2E testing
- [Acceptance Tests README](acceptance/README.md) - Language wrapper testing
- [Go Wrapper README](golang/README.md) - Go API documentation
- [TypeScript Wrapper README](typescript/README.md) - TypeScript API documentation
