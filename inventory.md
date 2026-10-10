# Inventory Specification

This document describes the `index.json` file format output by the recording mode of HTTP Playback Proxy.

## Overview

When recording mode completes (via SIGTERM/SIGINT), it saves an `index.json` file to the inventory directory. This file contains metadata about all recorded HTTP resources and is used by playback mode to replay the traffic with accurate timing.

## File Structure

```
<inventory_dir>/
├── index.json          # Main inventory file
├── contents/           # Editable response bodies (decoded, UTF-8 for text)
│   └── get/
│       └── https/
│           └── example.com/
│               └── path/
│                   └── file.html
└── originals/          # Bodies exactly as they crossed the wire (format 2)
    └── get/https/example.com/path/file.html
```

`originals/` holds a copy only for resources whose editable file differs from
the wire body (compressed, converted from another charset, or reformatted).
For the rest — an uncompressed image, say — the editable file is the wire body.

## Schema

### Inventory (Root Object)

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `formatVersion` | number | No | `2` for inventories written by 0.11 and later. Absent means the legacy format: see [Playback](#playback) |
| `entryUrl` | string | No | The initial URL used to start recording |
| `deviceType` | string | No | Device type used for recording: `"desktop"` or `"mobile"` |
| `resources` | Resource[] | Yes | Array of recorded HTTP resources |

### Resource

Each resource represents a single HTTP request/response pair.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `method` | string | Yes | HTTP method (e.g., `"GET"`, `"POST"`) |
| `url` | string | Yes | Full URL of the request |
| `ttfbMs` | number | Yes | Time from this request's start to its first response byte, in milliseconds. Replayed as a per-request wait |
| `durationMs` | number | No | Body download time in milliseconds (rounded), from the first to the last byte |
| `mbps` | number | No | Body throughput in megabits per second, from the wire bytes and the download time in microseconds |
| `statusCode` | number | No | HTTP response status code |
| `errorMessage` | string | No | Error message if request failed |
| `rawHeaders` | object | No | Response headers (see [Headers](#headers)) |
| `contentEncoding` | string | No | Original content encoding: `"gzip"`, `"deflate"`, `"br"`, `"compress"`, or `"identity"` |
| `contentTypeMime` | string | No | MIME type extracted from Content-Type header |
| `contentCharset` | string | No | Character encoding (e.g., `"utf-8"`, `"shift_jis"`) |
| `contentFilePath` | string | No | Path to saved content file (relative to inventory dir) |
| `contentUtf8` | string | No | Inline text body, used only when `contentFilePath` is absent or the file does not exist |
| `contentBase64` | string | No | Inline binary body, same rule. Not written since 0.11 — the file is the only copy |
| `minify` | boolean | No | `true` if the content was detected as minified (JS/CSS); playback minifies the editable file again |
| `original` | object | No | The recorded wire body and the state it was derived from — see [Original body](#original-body) |

### Original body

| Field | Type | Description |
|-------|------|-------------|
| `filePath` | string | Path of the wire body relative to the inventory directory (`originals/…`), still content-encoded |
| `contentSha256` | string | SHA-256 (hex) of the editable file as it was written |
| `contentEncoding`, `contentCharset`, `minify` | | The resource's values at recording time |

Playback serves `filePath` byte for byte, with the recorded headers, as long as
the editable file still hashes to `contentSha256` and the resource's
`contentEncoding`, `contentCharset` and `minify` equal the recorded ones.
Otherwise the body is regenerated from the editable file.

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

- `durationMs`: Time from receiving the first response byte to receiving the last byte, rounded to the millisecond
- `mbps`: `wire body bytes × 8 / download microseconds` (bits per microsecond are megabits per second)
- Uses the compressed (wire) body size, not the decompressed size
- Measured in microseconds so that a body that arrived in under a millisecond still has a throughput

### Playback

| `formatVersion` | Transfer time of a body |
|---|---|
| `2` | `wire bytes × 8 / mbps`. Without `mbps`, the median `mbps` of the same host |
| absent (legacy) | the recorded `durationMs`, regardless of the body size |

With format 2, an unedited body takes exactly the recorded time, a smaller one
proportionally less, and raising `mbps` models a faster link. The body is sent
in 64 KB chunks spread evenly over that time, after the `ttfbMs` wait.

## Content Processing

### Text Resources

Text resources (HTML, CSS, JavaScript) undergo special processing:

1. **Decompression**: Content is decompressed if encoded (gzip, deflate, brotli)
2. **Charset Conversion**: Content is converted to UTF-8
   - Charset is detected from Content-Type header or content declarations
   - After conversion, `contentCharset` reflects the original encoding
3. **Beautification** (JavaScript and CSS only): formatted with swc / lightningcss
4. **Minification Detection**: If the formatted line count is 2x+ the original, `minify: true` is set and the formatted copy is saved; otherwise the content is saved as received
5. **HTML is saved as received**, never reformatted: re-serialising a document changes whitespace between inline elements, character references and `<pre>` blocks
6. **The wire body** is kept under `originals/` (see [Original body](#original-body))

When an edited resource has `minify: true`, playback minifies it with the same
parser that formatted it, so automatic semicolon insertion, trailing comments
and template literals survive.

### Binary Resources

Binary resources (images, fonts, etc.) are:
1. Decompressed if encoded
2. Saved as-is to the contents directory (and the wire body to `originals/` if it was encoded)

## Content File Path Generation

URLs are converted to file paths using these rules:

- Base path: `contents/<method, lower-case>/<PROTOCOL>/<HOST>/<PATH>`
- Index handling: `/` becomes `/index.html`
- Query parameters:
  - If total length ≤ 32 chars: `resource~param=value.html`
  - If total length > 32 chars: `resource~first32chars.~<sha1_hash>.html`

### Examples

| URL | File Path |
|-----|-----------|
| `https://example.com/` | `contents/get/https/example.com/index.html` |
| `https://example.com/style.css` | `contents/get/https/example.com/style.css` |
| `https://example.com/api?id=1` | `contents/get/https/example.com/api~id%3D1` |

## Example

```json
{
  "formatVersion": 2,
  "entryUrl": "https://example.com/",
  "deviceType": "mobile",
  "resources": [
    {
      "method": "GET",
      "url": "https://example.com/",
      "ttfbMs": 120,
      "durationMs": 4,
      "mbps": 12.5,
      "statusCode": 200,
      "rawHeaders": {
        "content-type": "text/html; charset=utf-8",
        "content-encoding": "gzip",
        "cache-control": "max-age=3600",
        "set-cookie": ["a=1", "b=2"]
      },
      "contentEncoding": "gzip",
      "contentTypeMime": "text/html",
      "contentCharset": "utf-8",
      "contentFilePath": "contents/get/https/example.com/index.html",
      "minify": false,
      "original": {
        "filePath": "originals/get/https/example.com/index.html",
        "contentSha256": "9f2c…",
        "contentEncoding": "gzip",
        "contentCharset": "utf-8",
        "minify": false
      }
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
      "contentFilePath": "contents/get/https/example.com/style.css",
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
      "contentFilePath": "contents/get/https/example.com/logo.png"
    }
  ]
}
```

## TypeScript Type Definitions

For TypeScript/JavaScript integration, see `reference/types.ts`:

```typescript
export interface Inventory {
  formatVersion?: number;
  entryUrl?: string;
  extraUrls?: string[];
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
  original?: OriginalBody;
}

export interface OriginalBody {
  filePath: string;
  contentSha256: string;
  contentEncoding?: ContentEncodingType;
  contentCharset?: string;
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
