export type HttpHeaders = { [key: string]: string | string[] };

export type ContentEncodingType =
  | "gzip"
  | "compress"
  | "deflate"
  | "br"
  | "zstd"
  | "identity";

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

export interface Inventory {
  formatVersion?: number;
  entryUrl?: string;
  extraUrls?: string[];
  deviceType?: DeviceType;
  resources: Resource[];
}

export interface BodyChunk {
  chunk: Buffer;
  targetTime: number;
}

export interface Transaction {
  method: string;
  url: string;
  ttfb: number;
  statusCode?: number;
  errorMessage?: string;
  rawHeaders?: HttpHeaders;
  chunks: BodyChunk[];
}
