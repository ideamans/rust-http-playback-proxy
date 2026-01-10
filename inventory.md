# Inventory Specification

This document describes the `index.json` file format output by the recording mode of HTTP Playback Proxy.

## Overview

When recording mode completes (via SIGTERM/SIGINT), it saves an `index.json` file to the inventory directory. This file contains metadata about all recorded HTTP resources and is used by playback mode to replay the traffic with accurate timing.

## File Structure

```
<inventory_dir>/
├── index.json          # Main inventory file
└── contents/           # Directory containing saved response bodies
    └── GET/
        └── https/
            └── example.com/
                └── path/
                    └── file.html
```

## Schema

### Inventory (Root Object)

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `entryUrl` | string | No | The initial URL used to start recording |
| `deviceType` | string | No | Device type used for recording: `"desktop"` or `"mobile"` |
| `resources` | Resource[] | Yes | Array of recorded HTTP resources |

### Resource

Each resource represents a single HTTP request/response pair.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `method` | string | Yes | HTTP method (e.g., `"GET"`, `"POST"`) |
| `url` | string | Yes | Full URL of the request |
| `ttfbMs` | number | Yes | Time to First Byte in milliseconds (relative to first request) |
| `durationMs` | number | No | Transfer duration in milliseconds (from TTFB to response completion) |
| `mbps` | number | No | Transfer speed in megabits per second |
| `statusCode` | number | No | HTTP response status code |
| `errorMessage` | string | No | Error message if request failed |
| `rawHeaders` | object | No | Response headers (see [Headers](#headers)) |
| `contentEncoding` | string | No | Original content encoding: `"gzip"`, `"deflate"`, `"br"`, `"compress"`, or `"identity"` |
| `contentTypeMime` | string | No | MIME type extracted from Content-Type header |
| `contentCharset` | string | No | Character encoding (e.g., `"utf-8"`, `"shift_jis"`) |
| `contentFilePath` | string | No | Path to saved content file (relative to inventory dir) |
| `contentUtf8` | string | No | Text content inline (for small text resources) |
| `contentBase64` | string | No | Binary content as base64 (for binary resources) |
| `minify` | boolean | No | `true` if the content was detected as minified |

### Headers

Headers are stored as a key-value object where values can be either:
- A single string: `"Content-Type": "text/html"`
- An array of strings (for multi-value headers like Set-Cookie): `"Set-Cookie": ["a=1", "b=2"]`

```json
{
  "Content-Type": "text/html; charset=utf-8",
  "Set-Cookie": ["session=abc123", "user=john"],
  "Cache-Control": "no-cache"
}
```

### Content Encoding Types

| Value | Description |
|-------|-------------|
| `gzip` | Gzip compression |
| `deflate` | Deflate compression |
| `br` | Brotli compression |
| `compress` | LZW compression |
| `identity` | No compression |

## Timing Calculations

### TTFB (Time to First Byte)

- The first request received is treated as time origin (0 ms)
- All subsequent `ttfbMs` values are relative to this origin
- Represents the time from request start until the first byte of the response is received

### Duration and Mbps

- `durationMs`: Time from receiving the first response byte to receiving the last byte
- `mbps`: Calculated as `(compressed_body_bytes * 8) / (duration_seconds * 1,000,000)`
- Uses the compressed (wire) body size, not the decompressed size

## Content Processing

### Text Resources

Text resources (HTML, CSS, JavaScript) undergo special processing:

1. **Decompression**: Content is decompressed if encoded (gzip, deflate, brotli)
2. **Charset Conversion**: Content is converted to UTF-8
   - Charset is detected from Content-Type header or content declarations
   - After conversion, `contentCharset` reflects the original encoding
3. **Beautification**: Content is beautified (formatted with proper indentation)
4. **Minification Detection**: If beautified line count is 2x+ the original, `minify: true` is set

The beautified content is saved to enable easier editing for PageSpeed optimization.

### Binary Resources

Binary resources (images, fonts, etc.) are:
1. Decompressed if encoded
2. Saved as-is to the contents directory
3. Also stored as base64 in `contentBase64`

## Content File Path Generation

URLs are converted to file paths using these rules:

- Base path: `contents/<METHOD>/<PROTOCOL>/<HOST>/<PATH>`
- Index handling: `/` becomes `/index.html`
- Query parameters:
  - If total length ≤ 32 chars: `resource~param=value.html`
  - If total length > 32 chars: `resource~first32chars.~<sha1_hash>.html`

### Examples

| URL | File Path |
|-----|-----------|
| `https://example.com/` | `contents/GET/https/example.com/index.html` |
| `https://example.com/style.css` | `contents/GET/https/example.com/style.css` |
| `https://example.com/api?id=1` | `contents/GET/https/example.com/api~id=1` |

## Example

```json
{
  "entryUrl": "https://example.com/",
  "deviceType": "mobile",
  "resources": [
    {
      "method": "GET",
      "url": "https://example.com/",
      "ttfbMs": 0,
      "durationMs": 150,
      "mbps": 12.5,
      "statusCode": 200,
      "rawHeaders": {
        "Content-Type": "text/html; charset=utf-8",
        "Content-Encoding": "gzip",
        "Cache-Control": "max-age=3600"
      },
      "contentEncoding": "gzip",
      "contentTypeMime": "text/html",
      "contentCharset": "utf-8",
      "contentFilePath": "contents/GET/https/example.com/index.html",
      "minify": true
    },
    {
      "method": "GET",
      "url": "https://example.com/style.css",
      "ttfbMs": 50,
      "durationMs": 80,
      "mbps": 8.2,
      "statusCode": 200,
      "rawHeaders": {
        "Content-Type": "text/css",
        "Content-Encoding": "br"
      },
      "contentEncoding": "br",
      "contentTypeMime": "text/css",
      "contentFilePath": "contents/GET/https/example.com/style.css",
      "minify": false
    },
    {
      "method": "GET",
      "url": "https://example.com/logo.png",
      "ttfbMs": 100,
      "durationMs": 200,
      "mbps": 5.0,
      "statusCode": 200,
      "rawHeaders": {
        "Content-Type": "image/png"
      },
      "contentTypeMime": "image/png",
      "contentFilePath": "contents/GET/https/example.com/logo.png",
      "contentBase64": "iVBORw0KGgoAAAANSUhEUgAA..."
    }
  ]
}
```

## TypeScript Type Definitions

For TypeScript/JavaScript integration, see `reference/types.ts`:

```typescript
export interface Inventory {
  entryUrl?: string;
  deviceType?: DeviceType;
  resources: Resource[];
}

export interface Resource {
  method: string;
  url: string;
  ttfbMs: number;
  durationMs?: number;
  mbps?: number;
  statusCode?: number;
  errorMessage?: string;
  rawHeaders?: HttpHeaders;
  contentEncoding?: ContentEncodingType;
  contentTypeMime?: string;
  contentCharset?: string;
  contentFilePath?: string;
  contentUtf8?: string;
  contentBase64?: string;
  minify?: boolean;
}

export type DeviceType = "desktop" | "mobile";
export type ContentEncodingType = "gzip" | "compress" | "deflate" | "br" | "identity";
export type HttpHeaders = { [key: string]: string | string[] };
```

## Playback Mode Usage

When playback mode loads the inventory:

1. Reads `index.json` from the inventory directory
2. Converts each Resource to a Transaction (internal format)
3. If `minify: true`, re-minifies the content
4. Re-encodes content (gzip/brotli) as specified
5. Splits content into chunks with target timestamps
6. Replays responses with timing that matches the original recording
