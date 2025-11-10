export type HttpHeaders = { [key: string]: string };

export type ContentEncodingType =
  | "gzip"
  | "compress"
  | "deflate"
  | "br"
  | "identity";

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

export type DeviceType = "desktop" | "mobile";

export interface Inventory {
  entryUrl?: string;
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
