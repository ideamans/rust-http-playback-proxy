use clap::ValueEnum;
use hyper::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

/// HTTP header value that can be either a single string or multiple strings (for headers like Set-Cookie)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum HeaderValue {
    Single(String),
    Multiple(Vec<String>),
}

impl HeaderValue {
    /// Create a single-value header
    #[allow(dead_code)]
    pub fn single(value: String) -> Self {
        HeaderValue::Single(value)
    }

    /// Create a multi-value header
    #[allow(dead_code)]
    pub fn multiple(values: Vec<String>) -> Self {
        HeaderValue::Multiple(values)
    }

    /// Get the first value (useful for single-value headers)
    #[allow(dead_code)]
    pub fn first(&self) -> &str {
        match self {
            HeaderValue::Single(s) => s,
            HeaderValue::Multiple(v) => v.first().map(|s| s.as_str()).unwrap_or(""),
        }
    }

    /// Get all values as a vector
    pub fn as_vec(&self) -> Vec<&str> {
        match self {
            HeaderValue::Single(s) => vec![s.as_str()],
            HeaderValue::Multiple(v) => v.iter().map(|s| s.as_str()).collect(),
        }
    }
}

pub type HttpHeaders = HashMap<String, HeaderValue>;

/// HTTP protocol version
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum HttpVersion {
    #[serde(rename = "HTTP/0.9")]
    Http09,
    #[serde(rename = "HTTP/1.0")]
    Http10,
    #[serde(rename = "HTTP/1.1")]
    Http11,
    #[serde(rename = "HTTP/2")]
    Http2,
    #[serde(rename = "HTTP/3")]
    Http3,
}

impl From<Version> for HttpVersion {
    fn from(v: Version) -> Self {
        match v {
            Version::HTTP_09 => HttpVersion::Http09,
            Version::HTTP_10 => HttpVersion::Http10,
            Version::HTTP_11 => HttpVersion::Http11,
            Version::HTTP_2 => HttpVersion::Http2,
            Version::HTTP_3 => HttpVersion::Http3,
            _ => HttpVersion::Http11, // Default fallback
        }
    }
}

impl From<HttpVersion> for Version {
    fn from(v: HttpVersion) -> Self {
        match v {
            HttpVersion::Http09 => Version::HTTP_09,
            HttpVersion::Http10 => Version::HTTP_10,
            HttpVersion::Http11 => Version::HTTP_11,
            HttpVersion::Http2 => Version::HTTP_2,
            HttpVersion::Http3 => Version::HTTP_3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ContentEncodingType {
    Gzip,
    Compress,
    Deflate,
    Br,
    Zstd,
    Identity,
}

pub const SUPPORTED_ACCEPT_ENCODING: &str = "gzip, deflate, br, zstd";

impl FromStr for ContentEncodingType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "gzip" => Ok(ContentEncodingType::Gzip),
            "compress" => Ok(ContentEncodingType::Compress),
            "deflate" => Ok(ContentEncodingType::Deflate),
            "br" => Ok(ContentEncodingType::Br),
            "zstd" => Ok(ContentEncodingType::Zstd),
            "identity" => Ok(ContentEncodingType::Identity),
            _ => Err(format!("Unknown encoding type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub method: String,
    pub url: String,
    pub ttfb_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_version: Option<HttpVersion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mbps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_headers: Option<HttpHeaders>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_encoding: Option<ContentEncodingType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type_mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_charset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_utf8: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minify: Option<bool>,
    /// The body exactly as it crossed the wire at recording time, and the state of
    /// the editable file it was derived from. Playback serves these bytes untouched
    /// as long as the editable file and the fields that shape the body still match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original: Option<OriginalBody>,

    // Raw body bytes (as received from upstream, possibly compressed)
    // This field is used only during recording and is not serialized to index.json
    #[serde(skip)]
    pub raw_body: Option<Vec<u8>>,
}

/// The recorded wire body of a resource, kept so that an unedited resource is
/// replayed byte for byte (content, compression and size), not regenerated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OriginalBody {
    /// Path of the raw body relative to the inventory directory, still
    /// content-encoded exactly as received (starts with `originals/`).
    pub file_path: String,
    /// SHA-256 (hex) of the editable file under `contents/` as it was written.
    pub content_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_encoding: Option<ContentEncodingType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_charset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minify: Option<bool>,
}

/// Inventory format written by the current recorder.
///
/// - absent / 1: transfer time is the recorded `durationMs`, whatever the body size
/// - 2: transfer time is `wire bytes × 8 / mbps`, and unedited bodies are served
///   from `original`
pub const CURRENT_FORMAT_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Desktop,
    Mobile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_urls: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_type: Option<DeviceType>,
    pub resources: Vec<Resource>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BodyChunk {
    pub chunk: Vec<u8>,
    pub target_time: u64,
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub method: String,
    pub url: String,
    #[allow(dead_code)]
    // TODO: Will be used for timing validation once playback timing issues are fixed
    pub ttfb: u64,
    pub status_code: Option<u16>,
    pub error_message: Option<String>,
    pub raw_headers: Option<HttpHeaders>,
    pub chunks: Vec<BodyChunk>,
    pub target_close_time: u64, // Ideal connection close time in ms
}

impl Resource {
    pub fn new(method: String, url: String) -> Self {
        Self {
            method,
            url,
            ttfb_ms: 0,
            http_version: None,
            duration_ms: None,
            mbps: None,
            status_code: None,
            error_message: None,
            raw_headers: None,
            content_encoding: None,
            content_type_mime: None,
            content_charset: None,
            content_file_path: None,
            content_utf8: None,
            content_base64: None,
            minify: None,
            original: None,
            raw_body: None,
        }
    }
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            format_version: None,
            entry_url: None,
            extra_urls: None,
            device_type: None,
            resources: Vec::new(),
        }
    }
}

impl Default for Inventory {
    fn default() -> Self {
        Self::new()
    }
}

mod tests;
