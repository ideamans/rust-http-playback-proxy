/**
 * Device type for recording
 */
export type DeviceType = 'desktop' | 'mobile';

/**
 * Content encoding type
 */
export type ContentEncodingType = 'gzip' | 'compress' | 'deflate' | 'br' | 'identity';

/**
 * HTTP headers map
 */
export type HttpHeaders = Record<string, string>;

/**
 * Represents a single HTTP resource in the inventory
 */
export interface Resource {
  method: string;
  url: string;
  ttfbMs: number;
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

/**
 * Represents the complete inventory of recorded resources
 */
export interface Inventory {
  entryUrl?: string;
  deviceType?: DeviceType;
  resources: Resource[];
}

/**
 * Options for starting a recording proxy
 */
export interface RecordingOptions {
  entryUrl?: string;        // Optional: Entry URL to start recording from
  port?: number;            // Optional: Port to use (default: 18080, will auto-search)
  deviceType?: DeviceType;  // Optional: Device type (default: 'mobile')
  inventoryDir?: string;    // Optional: Inventory directory (default: './inventory')
  controlPort?: number;     // Optional: Control/management API port (enables HTTP shutdown)
}

/**
 * Options for starting a playback proxy
 */
export interface PlaybackOptions {
  port?: number;
  inventoryDir?: string;
  controlPort?: number;     // Optional: Control/management API port (enables HTTP shutdown)
}

/**
 * Proxy mode
 */
export type ProxyMode = 'recording' | 'playback';
