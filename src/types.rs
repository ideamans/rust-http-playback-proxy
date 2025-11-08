use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use clap::ValueEnum;

pub type HttpHeaders = HashMap<String, String>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ContentEncodingType {
    Gzip,
    Compress,
    Deflate,
    Br,
    Identity,
}

impl FromStr for ContentEncodingType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "gzip" => Ok(ContentEncodingType::Gzip),
            "compress" => Ok(ContentEncodingType::Compress),
            "deflate" => Ok(ContentEncodingType::Deflate),
            "br" => Ok(ContentEncodingType::Br),
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
    pub download_end_ms: Option<u64>,
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
    pub content_type_charset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_utf8: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minify: Option<bool>,
}

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
    pub entry_url: Option<String>,
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
    #[allow(dead_code)] // TODO: Will be used for timing validation once playback timing issues are fixed
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
            download_end_ms: None,
            mbps: None,
            status_code: None,
            error_message: None,
            raw_headers: None,
            content_encoding: None,
            content_type_mime: None,
            content_type_charset: None,
            content_file_path: None,
            content_utf8: None,
            content_base64: None,
            minify: None,
        }
    }
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            entry_url: None,
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