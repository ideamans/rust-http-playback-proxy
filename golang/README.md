# HTTP Playback Proxy - Go Wrapper

Go wrapper for the HTTP Playback Proxy Rust binary.

## Installation

```bash
go get github.com/pagespeed-quest/http-playback-proxy/golang
```

## Usage

### Recording Mode

```go
package main

import (
    "fmt"
    "time"

    proxy "github.com/pagespeed-quest/http-playback-proxy/golang"
)

func main() {
    // Start recording proxy (with entry URL)
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

    // Do your HTTP requests through the proxy (e.g., with a browser)
    // ...

    // Stop recording (this saves the inventory)
    time.Sleep(10 * time.Second) // Simulate some recording time

    if err := p.Stop(); err != nil {
        panic(err)
    }

    // Load the recorded inventory
    inventory, err := p.GetInventory()
    if err != nil {
        panic(err)
    }

    fmt.Printf("Recorded %d resources\n", len(inventory.Resources))
}

// Example: Start recording without entry URL (manual browsing)
func recordWithoutEntryURL() {
    // All options are optional - uses defaults
    p, err := proxy.StartRecording(proxy.RecordingOptions{})
    if err != nil {
        panic(err)
    }

    fmt.Printf("Recording proxy started on port %d\n", p.Port)
    fmt.Println("Configure your browser to use proxy 127.0.0.1:8080")
    fmt.Println("Then browse to any website...")

    // Stop after some time
    time.Sleep(30 * time.Second)
    p.Stop()
}
```

### Playback Mode

```go
package main

import (
    "fmt"
    "time"

    proxy "github.com/pagespeed-quest/http-playback-proxy/golang"
)

func main() {
    // Start playback proxy
    p, err := proxy.StartPlayback(proxy.PlaybackOptions{
        Port:         8080,
        InventoryDir: "./inventory",
    })
    if err != nil {
        panic(err)
    }

    fmt.Printf("Playback proxy started on port %d\n", p.Port)

    // Do your HTTP requests through the proxy
    // The proxy will replay the recorded responses with accurate timing
    // ...

    // Stop playback
    time.Sleep(10 * time.Second) // Simulate some playback time

    if err := p.Stop(); err != nil {
        panic(err)
    }
}
```

### Working with Inventory

```go
package main

import (
    "fmt"

    proxy "github.com/pagespeed-quest/http-playback-proxy/golang"
)

func main() {
    // Load inventory
    inventory, err := proxy.LoadInventory("./inventory/inventory.json")
    if err != nil {
        panic(err)
    }

    // Iterate through resources
    for i, resource := range inventory.Resources {
        fmt.Printf("Resource %d: %s %s\n", i, resource.Method, resource.URL)
        fmt.Printf("  TTFB: %d ms\n", resource.TtfbMs)
        if resource.StatusCode != nil {
            fmt.Printf("  Status: %d\n", *resource.StatusCode)
        }

        // Get content file path
        if resource.ContentFilePath != nil {
            contentPath := proxy.GetResourceContentPath("./inventory", &resource)
            fmt.Printf("  Content: %s\n", contentPath)
        }
    }
}
```

## API Reference

### Types

#### `RecordingOptions`
- `EntryURL` (string, optional): The entry URL to record
- `Port` (int, optional): Port to listen on (default: 8080, will auto-search)
- `DeviceType` (DeviceType, optional): Device type (desktop or mobile, default: mobile)
- `InventoryDir` (string, optional): Directory to save inventory (default: ./inventory)

#### `PlaybackOptions`
- `Port` (int, optional): Port to listen on (default: 8080, will auto-search)
- `InventoryDir` (string, optional): Directory containing inventory (default: ./inventory)

#### `Inventory`
- `EntryURL` (*string): The entry URL that was recorded
- `DeviceType` (*DeviceType): Device type used for recording
- `Resources` ([]Resource): List of recorded resources

#### `Resource`
- `Method` (string): HTTP method
- `URL` (string): Resource URL
- `TtfbMs` (uint64): Time to first byte in milliseconds
- `Mbps` (*float64): Transfer speed in megabits per second
- `StatusCode` (*uint16): HTTP status code
- `ContentFilePath` (*string): Path to content file (relative to inventory dir)
- And more...

### Functions

#### `StartRecording(opts RecordingOptions) (*Proxy, error)`
Starts a recording proxy.

#### `StartPlayback(opts PlaybackOptions) (*Proxy, error)`
Starts a playback proxy.

#### `(*Proxy).Stop() error`
Stops the proxy gracefully.

#### `(*Proxy).IsRunning() bool`
Checks if the proxy is still running.

#### `(*Proxy).GetInventory() (*Inventory, error)`
Loads the inventory for this proxy.

#### `LoadInventory(path string) (*Inventory, error)`
Loads an inventory from a JSON file.

#### `SaveInventory(path string, inventory *Inventory) error`
Saves an inventory to a JSON file.

#### `GetResourceContentPath(inventoryDir string, resource *Resource) string`
Returns the full path to a resource's content file.

#### `EnsureBinary() error`
Ensures the binary is available, downloading if necessary.

## Binary Distribution

### How Binaries are Located

The Go wrapper searches for binaries in the following order:

1. **Package directory** (`golang/bin/<platform>/`) - Development only
   - Only works when developing in the source tree
   - Go modules are installed read-only in `$GOPATH/pkg/mod`, so binaries cannot be bundled
2. **Cache directory** (`~/.cache/http-playback-proxy/bin/<platform>/`) - Production
   - Binaries are automatically downloaded on first use
   - Can be customized via `HTTP_PLAYBACK_PROXY_CACHE_DIR` environment variable

### First-Time Usage

When you first use the proxy, it will automatically download the appropriate binary for your platform:

```go
import proxy "github.com/pagespeed-quest/http-playback-proxy/golang"

func main() {
    // Automatically downloads binary if not found
    p, err := proxy.StartRecording(proxy.RecordingOptions{})
    if err != nil {
        panic(err)
    }
    // ...
}
```

You can also explicitly download the binary:

```go
if err := proxy.EnsureBinary(); err != nil {
    panic(err)
}
```
