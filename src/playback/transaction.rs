use crate::traits::FileSystem;
use crate::types::{BodyChunk, Inventory, Resource, Transaction};
use anyhow::Result;
use encoding_rs::{Encoding, UTF_8};
use std::path::Path;
use std::sync::Arc;

const CHUNK_SIZE: usize = 1024 * 64; // 64KB chunks
const TARGET_MBPS: f64 = 1.0; // Default target speed in Mbps

pub async fn convert_resources_to_transactions<F: FileSystem>(
    inventory: &Inventory,
    inventory_dir: &Path,
    file_system: Arc<F>,
) -> Result<Vec<Transaction>> {
    let mut transactions = Vec::new();

    for resource in &inventory.resources {
        if let Some(transaction) =
            convert_resource_to_transaction(resource, inventory_dir, file_system.clone()).await?
        {
            transactions.push(transaction);
        }
    }

    Ok(transactions)
}

pub async fn convert_resource_to_transaction<F: FileSystem>(
    resource: &Resource,
    inventory_dir: &Path,
    file_system: Arc<F>,
) -> Result<Option<Transaction>> {
    // Load content
    let content = if let Some(file_path) = &resource.content_file_path {
        // file_path is now relative to inventory_dir (includes "contents/" prefix)
        let full_path = inventory_dir.join(file_path);
        if file_system.exists(&full_path).await {
            file_system.read(&full_path).await?
        } else if let Some(base64_content) = &resource.content_base64 {
            use base64::{Engine as _, engine::general_purpose};
            general_purpose::STANDARD.decode(base64_content)?
        } else if let Some(utf8_content) = &resource.content_utf8 {
            utf8_content.as_bytes().to_vec()
        } else {
            return Ok(None);
        }
    } else if let Some(base64_content) = &resource.content_base64 {
        use base64::{Engine as _, engine::general_purpose};
        general_purpose::STANDARD.decode(base64_content)?
    } else if let Some(utf8_content) = &resource.content_utf8 {
        utf8_content.as_bytes().to_vec()
    } else {
        return Ok(None);
    };

    // Process content based on minify flag
    let mut processed_content = if resource.minify.unwrap_or(false) {
        minify_content(&content, &resource.content_type_mime)?
    } else {
        content
    };

    // Re-encode to original charset if this is a text resource with original_charset
    if let Some(original_charset) = &resource.original_charset {
        processed_content = re_encode_to_charset(&processed_content, original_charset)?;
    }

    // Compress content if needed
    let final_content = if let Some(encoding) = &resource.content_encoding {
        compress_content(&processed_content, encoding)?
    } else {
        processed_content
    };

    // Create chunks and calculate target_close_time
    let (chunks, target_close_time) = create_chunks(&final_content, resource)?;

    let mut headers = resource.raw_headers.clone().unwrap_or_default();

    // Update content-length
    headers.insert(
        "content-length".to_string(),
        crate::types::HeaderValue::Single(final_content.len().to_string()),
    );

    // Update charset - use original_charset if available, otherwise fall back to content_type_charset
    if let Some(mime_type) = &resource.content_type_mime {
        let charset_to_use = resource
            .original_charset
            .as_ref()
            .or(resource.content_type_charset.as_ref());

        let content_type_value = if let Some(charset) = charset_to_use {
            format!("{}; charset={}", mime_type, charset)
        } else {
            mime_type.clone()
        };

        headers.insert(
            "content-type".to_string(),
            crate::types::HeaderValue::Single(content_type_value),
        );
    }

    Ok(Some(Transaction {
        method: resource.method.clone(),
        url: resource.url.clone(),
        ttfb: resource.ttfb_ms,
        status_code: resource.status_code,
        error_message: resource.error_message.clone(),
        raw_headers: Some(headers),
        chunks,
        target_close_time,
    }))
}

pub fn create_chunks(content: &[u8], resource: &Resource) -> Result<(Vec<BodyChunk>, u64)> {
    let mut chunks = Vec::new();
    let total_size = content.len();

    if total_size == 0 {
        // If no content, close time is 0 (TTFB is handled separately in serve_transaction)
        return Ok((chunks, 0));
    }

    // Use actual recorded transfer duration (download_end_ms - ttfb_ms)
    // This ensures we reproduce the exact timing from the recording
    let transfer_duration_ms = if let Some(download_end_ms) = resource.download_end_ms {
        download_end_ms.saturating_sub(resource.ttfb_ms)
    } else {
        // Fallback: calculate from mbps if download_end_ms is not available
        let mbps = resource.mbps.unwrap_or(TARGET_MBPS);
        let bytes_per_ms = (mbps * 1000.0 * 1000.0) / 8.0 / 1000.0;
        (total_size as f64 / bytes_per_ms) as u64
    };

    // If transfer duration is 0, make it at least 1ms to avoid division by zero
    let transfer_duration_ms = std::cmp::max(1, transfer_duration_ms);

    let mut offset = 0;
    // Start at 0 - chunks are relative times from TTFB (TTFB is waited separately in proxy.rs)
    let mut current_time = 0u64;

    while offset < total_size {
        let chunk_size = std::cmp::min(CHUNK_SIZE, total_size - offset);
        let chunk_data = content[offset..offset + chunk_size].to_vec();

        chunks.push(BodyChunk {
            chunk: chunk_data,
            target_time: current_time,
        });

        // Calculate time for next chunk based on proportional distribution
        // Each chunk gets its share of the total transfer time based on its size
        let chunk_duration_ms =
            ((chunk_size as f64 / total_size as f64) * transfer_duration_ms as f64) as u64;
        current_time += chunk_duration_ms;
        offset += chunk_size;
    }

    // target_close_time is the total transfer duration (relative to TTFB completion)
    let target_close_time = transfer_duration_ms;

    Ok((chunks, target_close_time))
}

pub fn minify_content(content: &[u8], mime_type: &Option<String>) -> Result<Vec<u8>> {
    let content_str = String::from_utf8_lossy(content);

    let minified = match mime_type.as_deref() {
        Some("text/html") => {
            // Simple HTML minification - remove extra whitespace
            let mut result = String::new();
            let mut in_tag = false;
            let mut prev_was_space = false;

            for ch in content_str.chars() {
                match ch {
                    '<' => {
                        in_tag = true;
                        result.push(ch);
                        prev_was_space = false;
                    }
                    '>' => {
                        in_tag = false;
                        result.push(ch);
                        prev_was_space = false;
                    }
                    '\n' | '\r' | '\t' | ' ' => {
                        if !in_tag && !prev_was_space {
                            result.push(' ');
                            prev_was_space = true;
                        } else if in_tag {
                            result.push(ch);
                        }
                    }
                    _ => {
                        result.push(ch);
                        prev_was_space = false;
                    }
                }
            }
            result
        }
        Some("text/css") => {
            // Simple CSS minification
            content_str
                .lines()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("")
        }
        Some("application/javascript") | Some("text/javascript") => {
            // Simple JS minification - remove extra whitespace and newlines
            content_str
                .lines()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty() && !line.starts_with("//"))
                .collect::<Vec<_>>()
                .join("")
        }
        _ => content_str.to_string(),
    };

    Ok(minified.into_bytes())
}

use crate::types::ContentEncodingType;

pub fn compress_content(content: &[u8], encoding: &ContentEncodingType) -> Result<Vec<u8>> {
    match encoding {
        ContentEncodingType::Gzip => {
            use flate2::Compression;
            use flate2::write::GzEncoder;
            use std::io::Write;

            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(content)?;
            Ok(encoder.finish()?)
        }
        ContentEncodingType::Deflate => {
            use flate2::Compression;
            use flate2::write::DeflateEncoder;
            use std::io::Write;

            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(content)?;
            Ok(encoder.finish()?)
        }
        ContentEncodingType::Br => {
            let mut compressed = Vec::new();
            brotli::BrotliCompress(
                &mut std::io::Cursor::new(content),
                &mut compressed,
                &Default::default(),
            )?;
            Ok(compressed)
        }
        _ => Ok(content.to_vec()),
    }
}

pub fn re_encode_to_charset(content: &[u8], charset_name: &str) -> Result<Vec<u8>> {
    // File content is stored as UTF-8, convert it back to original charset
    let utf8_str = std::str::from_utf8(content)?;

    // Get the target encoding
    let encoding = Encoding::for_label(charset_name.as_bytes())
        .ok_or_else(|| anyhow::anyhow!("Unknown charset: {}", charset_name))?;

    // If target is UTF-8, no conversion needed
    if encoding == UTF_8 {
        return Ok(content.to_vec());
    }

    // Encode from UTF-8 to target charset
    let (encoded, _encoding_used, _had_errors) = encoding.encode(utf8_str);
    Ok(encoded.into_owned())
}
