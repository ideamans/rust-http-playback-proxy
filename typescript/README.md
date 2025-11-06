# HTTP Playback Proxy - TypeScript/Node.js Wrapper

TypeScript/Node.js wrapper for the HTTP Playback Proxy Rust binary.

## Installation

```bash
npm install http-playback-proxy
```

## Usage

### Recording Mode

```typescript
import { startRecording } from 'http-playback-proxy';

async function record() {
  // Start recording proxy (with entry URL)
  const proxy = await startRecording({
    entryUrl: 'https://example.com',
    port: 8080,
    deviceType: 'mobile',
    inventoryDir: './inventory',
  });

  console.log(`Recording proxy started on port ${proxy.port}`);

  // Do your HTTP requests through the proxy (e.g., with a browser)
  // ...

  // Stop recording after some time (this saves the inventory)
  setTimeout(async () => {
    await proxy.stop();

    // Load the recorded inventory
    const inventory = await proxy.getInventory();
    console.log(`Recorded ${inventory.resources.length} resources`);
  }, 10000);
}

// Example: Start recording without entry URL (manual browsing)
async function recordWithoutEntryURL() {
  // All options are optional - uses defaults
  const proxy = await startRecording({});

  console.log(`Recording proxy started on port ${proxy.port}`);
  console.log('Configure your browser to use proxy 127.0.0.1:8080');
  console.log('Then browse to any website...');

  // Stop after some time
  setTimeout(async () => {
    await proxy.stop();
  }, 30000);
}

record().catch(console.error);
```

### Playback Mode

```typescript
import { startPlayback } from 'http-playback-proxy';

async function playback() {
  // Start playback proxy
  const proxy = await startPlayback({
    port: 8080,
    inventoryDir: './inventory',
  });

  console.log(`Playback proxy started on port ${proxy.port}`);

  // Do your HTTP requests through the proxy
  // The proxy will replay the recorded responses with accurate timing
  // ...

  // Stop playback after some time
  setTimeout(async () => {
    await proxy.stop();
  }, 10000);
}

playback().catch(console.error);
```

### Working with Inventory

```typescript
import { loadInventory, getResourceContentPath } from 'http-playback-proxy';

async function analyzeInventory() {
  // Load inventory
  const inventory = await loadInventory('./inventory/inventory.json');

  // Iterate through resources
  for (const [i, resource] of inventory.resources.entries()) {
    console.log(`Resource ${i}: ${resource.method} ${resource.url}`);
    console.log(`  TTFB: ${resource.ttfbMs} ms`);
    if (resource.statusCode) {
      console.log(`  Status: ${resource.statusCode}`);
    }

    // Get content file path
    if (resource.contentFilePath) {
      const contentPath = getResourceContentPath('./inventory', resource);
      console.log(`  Content: ${contentPath}`);
    }
  }
}

analyzeInventory().catch(console.error);
```

## API Reference

### Types

#### `RecordingOptions`
```typescript
interface RecordingOptions {
  entryUrl?: string;       // Optional: Entry URL to start recording from
  port?: number;           // Optional: Port to listen on (default: 8080, will auto-search)
  deviceType?: DeviceType; // Optional: 'desktop' or 'mobile' (default: 'mobile')
  inventoryDir?: string;   // Optional: Directory to save inventory (default: './inventory')
}
```

#### `PlaybackOptions`
```typescript
interface PlaybackOptions {
  port?: number;         // Optional: Port to listen on (default: 8080, will auto-search)
  inventoryDir?: string; // Optional: Directory containing inventory (default: './inventory')
}
```

#### `Inventory`
```typescript
interface Inventory {
  entryUrl?: string;
  deviceType?: DeviceType;
  resources: Resource[];
}
```

#### `Resource`
```typescript
interface Resource {
  method: string;
  url: string;
  ttfbMs: number;
  mbps?: number;
  statusCode?: number;
  errorMessage?: string;
  rawHeaders?: Record<string, string>;
  contentEncoding?: ContentEncodingType;
  contentTypeMime?: string;
  contentTypeCharset?: string;
  contentFilePath?: string;
  contentUtf8?: string;
  contentBase64?: string;
  minify?: boolean;
}
```

### Functions

#### `startRecording(options: RecordingOptions): Promise<Proxy>`
Starts a recording proxy.

#### `startPlayback(options: PlaybackOptions): Promise<Proxy>`
Starts a playback proxy.

#### `Proxy.stop(): Promise<void>`
Stops the proxy gracefully.

#### `Proxy.isRunning(): boolean`
Checks if the proxy is still running.

#### `Proxy.wait(): Promise<void>`
Waits for the proxy to exit.

#### `Proxy.getInventory(): Promise<Inventory>`
Loads the inventory for this proxy.

#### `loadInventory(path: string): Promise<Inventory>`
Loads an inventory from a JSON file.

#### `saveInventory(path: string, inventory: Inventory): Promise<void>`
Saves an inventory to a JSON file.

#### `getResourceContentPath(inventoryDir: string, resource: Resource): string`
Returns the full path to a resource's content file.

#### `getInventoryPath(inventoryDir: string): string`
Returns the path to the inventory.json file.

#### `ensureBinary(): Promise<void>`
Ensures the binary is available, downloading if necessary.

## Binary Distribution

### How Binaries are Located

The TypeScript wrapper searches for binaries in the following order:

1. **Package directory** (`node_modules/http-playback-proxy/bin/<platform>/`) - Production
   - Binaries are bundled with the npm package via GitHub Actions
   - Included in the `files` field of package.json
2. **Cache directory** (`~/.cache/http-playback-proxy/bin/<platform>/`) - Fallback
   - Used if package binaries are not available
   - Automatically downloaded on first use
   - Can be customized via `HTTP_PLAYBACK_PROXY_CACHE_DIR` environment variable

### Package Contents

The npm package includes:
- `dist/` - Compiled TypeScript code
- `bin/` - Prebuilt binaries for all supported platforms
- `README.md` - Documentation

### First-Time Usage

The wrapper automatically ensures the binary is available:

```typescript
import { startRecording } from 'http-playback-proxy';

async function main() {
  // Automatically uses packaged binary or downloads if needed
  const proxy = await startRecording({});
  // ...
}
```

You can also explicitly ensure the binary is available:

```typescript
import { ensureBinary } from 'http-playback-proxy';

await ensureBinary();
```

#### `checkBinaryExists(): boolean`
Checks if the binary exists locally.

#### `downloadBinary(version?: string): Promise<void>`
Downloads a specific version of the binary from GitHub Releases.

## Environment Variables

- `HTTP_PLAYBACK_PROXY_CACHE_DIR`: Custom cache directory for downloaded binaries
- `XDG_CACHE_HOME`: XDG cache directory (fallback)

If neither is set, binaries are cached in `~/.cache/http-playback-proxy`.

## License

MIT
