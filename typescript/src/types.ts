/**
 * Device type for recording
 */
export type DeviceType = 'desktop' | 'mobile';

/**
 * Content encoding type
 */
export type ContentEncodingType = 'gzip' | 'compress' | 'deflate' | 'br' | 'zstd' | 'identity';

/**
 * HTTP headers map
 */
export type HttpHeaders = Record<string, string | string[]>;

/**
 * Represents a single HTTP resource in the inventory
 */
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

/**
 * The recorded wire body of a resource. Playback serves it untouched while the
 * editable file and the fields that shape the body still match.
 */
export interface OriginalBody {
  filePath: string;
  contentSha256: string;
  contentEncoding?: ContentEncodingType;
  contentCharset?: string;
  minify?: boolean;
}

/**
 * Represents the complete inventory of recorded resources
 */
export interface Inventory {
  /** 2: transfer time is wire bytes × 8 / mbps, unedited bodies replay from `original`. Absent: `durationMs`. */
  formatVersion?: number;
  entryUrl?: string;
  extraUrls?: string[];
  deviceType?: DeviceType;
  resources: Resource[];
}

/**
 * Options for starting a recording proxy
 */
export interface RecordingOptions {
  entryUrl?: string;        // Optional: Entry URL to start recording from
  extraUrls?: string[];     // Optional: Additional entry URLs
  port?: number;            // Optional: Port to use (default: 18080, will auto-search)
  deviceType?: DeviceType;  // Optional: Device type (default: 'mobile')
  inventoryDir?: string;    // Optional: Inventory directory (default: './inventory')
}

/**
 * Options for starting a playback proxy
 */
export interface PlaybackOptions {
  port?: number;
  inventoryDir?: string;
  fullThrottle?: boolean; // Optional: Disable timing control (TTFB and transfer speed) for fastest playback
  passthrough?: boolean; // Optional: Forward unmatched requests to real servers instead of returning 404
}

/**
 * Proxy mode
 */
export type ProxyMode = 'recording' | 'playback';
