use crate::traits::FileSystem;
use crate::types::{
    BodyChunk, CURRENT_FORMAT_VERSION, ContentEncodingType, Inventory, Resource, Transaction,
};
use anyhow::Result;
use encoding_rs::{Encoding, UTF_8};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

const CHUNK_SIZE: usize = 1024 * 64; // 64KB chunks
const TARGET_MBPS: f64 = 1.0; // Default target speed in Mbps

/// How the inventory being replayed wants bodies and timing reproduced.
#[derive(Debug, Clone, Default)]
pub struct PlaybackContext {
    /// `Inventory::format_version`, 1 when absent.
    pub format_version: u32,
    /// Median recorded throughput per host, for a resource of a current-format
    /// inventory that has no throughput of its own.
    pub host_mbps: HashMap<String, f64>,
}

impl PlaybackContext {
    #[cfg(test)]
    pub fn legacy() -> Self {
        Self {
            format_version: 1,
            host_mbps: HashMap::new(),
        }
    }

    pub fn from_inventory(inventory: &Inventory) -> Self {
        let mut samples: HashMap<String, Vec<f64>> = HashMap::new();
        for resource in &inventory.resources {
            if let (Some(host), Some(mbps)) = (host_of(&resource.url), resource.mbps)
                && mbps.is_finite()
                && mbps > 0.0
            {
                samples.entry(host).or_default().push(mbps);
            }
        }
        let host_mbps = samples
            .into_iter()
            .map(|(host, mut values)| {
                values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                (host, values[values.len() / 2])
            })
            .collect();
        Self {
            format_version: inventory.format_version.unwrap_or(1),
            host_mbps,
        }
    }

    fn sizes_drive_timing(&self) -> bool {
        self.format_version >= CURRENT_FORMAT_VERSION
    }
}

fn host_of(url: &str) -> Option<String> {
    url.parse::<hyper::Uri>()
        .ok()
        .and_then(|uri| uri.host().map(|h| h.to_ascii_lowercase()))
}

pub async fn convert_resources_to_transactions<F: FileSystem>(
    inventory: &Inventory,
    inventory_dir: &Path,
    file_system: Arc<F>,
) -> Result<Vec<Transaction>> {
    let context = PlaybackContext::from_inventory(inventory);
    let mut transactions = Vec::new();

    for resource in &inventory.resources {
        if let Some(transaction) = convert_resource_to_transaction_with(
            resource,
            inventory_dir,
            file_system.clone(),
            &context,
        )
        .await?
        {
            transactions.push(transaction);
        }
    }

    Ok(transactions)
}

/// Convert one resource as the legacy format does (timing from `durationMs`).
#[cfg(test)]
pub async fn convert_resource_to_transaction<F: FileSystem>(
    resource: &Resource,
    inventory_dir: &Path,
    file_system: Arc<F>,
) -> Result<Option<Transaction>> {
    convert_resource_to_transaction_with(
        resource,
        inventory_dir,
        file_system,
        &PlaybackContext::legacy(),
    )
    .await
}

pub async fn convert_resource_to_transaction_with<F: FileSystem>(
    resource: &Resource,
    inventory_dir: &Path,
    file_system: Arc<F>,
    context: &PlaybackContext,
) -> Result<Option<Transaction>> {
    let mut headers = resource.raw_headers.clone().unwrap_or_default();

    let final_content = if let Some(original) =
        unedited_original(resource, inventory_dir, file_system.as_ref()).await?
    {
        // Byte for byte what the browser received at recording time, with the
        // recorded Content-Encoding header that describes those bytes.
        original
    } else {
        let Some(content) =
            load_editable_content(resource, inventory_dir, file_system.as_ref()).await?
        else {
            return Ok(None);
        };
        let regenerated = regenerate_body(content, resource, context)?;
        set_content_encoding_header(&mut headers, &resource.content_encoding);
        regenerated
    };

    let (chunks, target_close_time) = if context.sizes_drive_timing() {
        let duration = transfer_duration_ms(final_content.len(), resource, context);
        create_chunks_with_duration(&final_content, duration)
    } else {
        create_chunks(&final_content, resource)?
    };

    // Update content-length
    headers.insert(
        "content-length".to_string(),
        crate::types::HeaderValue::Single(final_content.len().to_string()),
    );

    // NOTE: We do NOT modify Content-Type header here.
    // The original Content-Type from raw_headers is preserved exactly as recorded.
    // The content_charset field is only used for re-encoding the body content,
    // NOT for modifying HTTP headers.

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

/// The recorded wire body, if the resource has not been edited since recording:
/// the editable file still hashes to what was written, and none of the fields
/// that shape the served body (encoding, charset, minify) has changed.
async fn unedited_original<F: FileSystem>(
    resource: &Resource,
    inventory_dir: &Path,
    file_system: &F,
) -> Result<Option<Vec<u8>>> {
    let Some(original) = &resource.original else {
        return Ok(None);
    };
    if original.content_encoding != resource.content_encoding
        || original.content_charset != resource.content_charset
        || original.minify != resource.minify
    {
        return Ok(None);
    }
    let Some(content_path) = &resource.content_file_path else {
        return Ok(None);
    };
    let content_path = inventory_dir.join(content_path);
    let original_path = inventory_dir.join(&original.file_path);
    if !file_system.exists(&content_path).await || !file_system.exists(&original_path).await {
        return Ok(None);
    }
    let editable = file_system.read(&content_path).await?;
    if crate::utils::sha256_hex(&editable) != original.content_sha256 {
        return Ok(None);
    }
    Ok(Some(file_system.read(&original_path).await?))
}

/// The editable body: the file under `contents/` when it exists, otherwise an
/// inline `contentBase64` / `contentUtf8`.
async fn load_editable_content<F: FileSystem>(
    resource: &Resource,
    inventory_dir: &Path,
    file_system: &F,
) -> Result<Option<Vec<u8>>> {
    if let Some(file_path) = &resource.content_file_path {
        // file_path is relative to inventory_dir (includes "contents/" prefix)
        let full_path = inventory_dir.join(file_path);
        if file_system.exists(&full_path).await {
            return Ok(Some(file_system.read(&full_path).await?));
        }
    }
    if let Some(base64_content) = &resource.content_base64 {
        use base64::{Engine as _, engine::general_purpose};
        return Ok(Some(general_purpose::STANDARD.decode(base64_content)?));
    }
    if let Some(utf8_content) = &resource.content_utf8 {
        return Ok(Some(utf8_content.as_bytes().to_vec()));
    }
    Ok(None)
}

/// Build the wire body from an edited (or legacy) resource: minify if flagged,
/// re-encode to the original charset, then compress with `contentEncoding`.
fn regenerate_body(
    content: Vec<u8>,
    resource: &Resource,
    context: &PlaybackContext,
) -> Result<Vec<u8>> {
    // Process content based on minify flag
    // If minification fails, log warning and use original content
    let mut processed_content = if resource.minify.unwrap_or(false) {
        let minified = if context.sizes_drive_timing() {
            minify_content(&content, &resource.content_type_mime)
        } else {
            legacy_minify_content(&content, &resource.content_type_mime)
        };
        match minified {
            Ok(minified) => minified,
            Err(e) => {
                tracing::warn!(
                    "Failed to minify content for {}: {}. Using original content.",
                    resource.url,
                    e
                );
                content
            }
        }
    } else {
        content
    };

    // Re-encode to original charset if this is a text resource with content_charset
    // If re-encoding fails, log warning and keep UTF-8 content
    if let Some(charset) = &resource.content_charset {
        match re_encode_to_charset(&processed_content, charset) {
            Ok(reencoded) => {
                processed_content = reencoded;
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to re-encode content for {} to {}: {}. Keeping UTF-8 content.",
                    resource.url,
                    charset,
                    e
                );
            }
        }
    }

    // Compress content if needed
    if let Some(encoding) = &resource.content_encoding {
        compress_content(&processed_content, encoding)
    } else {
        Ok(processed_content)
    }
}

/// Make the Content-Encoding header describe the regenerated body, so that an
/// edited `contentEncoding` is not served under the recorded header.
fn set_content_encoding_header(
    headers: &mut crate::types::HttpHeaders,
    encoding: &Option<ContentEncodingType>,
) {
    if encoding.is_none() {
        // A recorded header we could not parse (x-gzip, a chain of codings) was
        // never decoded, so the body is still encoded that way: leave it alone.
        let recorded_is_known = headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("content-encoding")
                && value.first().parse::<ContentEncodingType>().is_ok()
        });
        if !recorded_is_known {
            return;
        }
    }
    headers.retain(|name, _| !name.eq_ignore_ascii_case("content-encoding"));
    let value = match encoding {
        Some(ContentEncodingType::Gzip) => "gzip",
        Some(ContentEncodingType::Deflate) => "deflate",
        Some(ContentEncodingType::Br) => "br",
        Some(ContentEncodingType::Zstd) => "zstd",
        Some(ContentEncodingType::Compress) => "compress",
        Some(ContentEncodingType::Identity) | None => return,
    };
    headers.insert(
        "content-encoding".to_string(),
        crate::types::HeaderValue::Single(value.to_string()),
    );
}

/// Milliseconds to transfer `wire_len` body bytes.
///
/// Current format: `wire bytes × 8 / mbps`. The recorded `mbps` comes from the
/// recorded wire bytes and the microsecond download time, so an unedited body
/// takes the recorded time and an edited one takes time in proportion to its
/// size. Without a throughput of its own, the median of the same host is used.
///
/// Legacy format: the recorded `durationMs`, whatever the size.
pub fn transfer_duration_ms(
    wire_len: usize,
    resource: &Resource,
    context: &PlaybackContext,
) -> u64 {
    if wire_len == 0 {
        return 0;
    }
    if !context.sizes_drive_timing() {
        return legacy_transfer_duration_ms(wire_len, resource);
    }
    let mbps = resource
        .mbps
        .filter(|m| m.is_finite() && *m > 0.0)
        .or_else(|| host_of(&resource.url).and_then(|h| context.host_mbps.get(&h).copied()))
        .unwrap_or(TARGET_MBPS);
    // bits / (megabits per second) = microseconds
    let micros = wire_len as f64 * 8.0 / mbps;
    (micros / 1000.0).round() as u64
}

fn legacy_transfer_duration_ms(total_size: usize, resource: &Resource) -> u64 {
    // Use actual recorded transfer duration (duration_ms)
    if let Some(duration_ms) = resource.duration_ms {
        duration_ms
    } else {
        // Fallback: calculate from mbps if duration_ms is not available
        let mbps = resource.mbps.unwrap_or(TARGET_MBPS);
        let bytes_per_ms = (mbps * 1000.0 * 1000.0) / 8.0 / 1000.0;
        (total_size as f64 / bytes_per_ms) as u64
    }
}

/// Split a body into chunks with the legacy timing (recorded `durationMs`).
pub fn create_chunks(content: &[u8], resource: &Resource) -> Result<(Vec<BodyChunk>, u64)> {
    if content.is_empty() {
        // If no content, close time is 0 (TTFB is handled separately in serve_transaction)
        return Ok((Vec::new(), 0));
    }
    // If transfer duration is 0, make it at least 1ms to avoid division by zero
    let transfer_duration_ms =
        std::cmp::max(1, legacy_transfer_duration_ms(content.len(), resource));
    let total_size = content.len();
    let mut chunks = Vec::new();
    let mut offset = 0;
    let mut current_time = 0u64;
    while offset < total_size {
        let chunk_size = std::cmp::min(CHUNK_SIZE, total_size - offset);
        chunks.push(BodyChunk {
            chunk: content[offset..offset + chunk_size].to_vec(),
            target_time: current_time,
        });
        // Legacy pacing: each chunk's share truncated on its own
        current_time +=
            ((chunk_size as f64 / total_size as f64) * transfer_duration_ms as f64) as u64;
        offset += chunk_size;
    }
    Ok((chunks, transfer_duration_ms))
}

/// Split a body into chunks spread evenly, by size, over `transfer_duration_ms`
/// counted from the end of the TTFB wait.
pub fn create_chunks_with_duration(
    content: &[u8],
    transfer_duration_ms: u64,
) -> (Vec<BodyChunk>, u64) {
    let mut chunks = Vec::new();
    let total_size = content.len();
    if total_size == 0 {
        return (chunks, 0);
    }

    let mut offset = 0;
    let mut current_time = 0u64;

    while offset < total_size {
        let chunk_size = std::cmp::min(CHUNK_SIZE, total_size - offset);
        chunks.push(BodyChunk {
            chunk: content[offset..offset + chunk_size].to_vec(),
            target_time: current_time,
        });

        // Cumulative share of the transfer time, so rounding never accumulates
        offset += chunk_size;
        current_time = ((offset as f64 / total_size as f64) * transfer_duration_ms as f64) as u64;
    }

    // target_close_time is the total transfer duration (relative to TTFB completion)
    (chunks, transfer_duration_ms)
}

/// Minify an edited text body with real parsers. HTML is never rewritten: the
/// recorder keeps it as recorded, and any textual whitespace removal can change
/// how the document renders.
pub fn minify_content(content: &[u8], mime_type: &Option<String>) -> Result<Vec<u8>> {
    let content_str = String::from_utf8_lossy(content);
    let minified = match mime_type.as_deref() {
        Some("text/css") => crate::beautify::minify_css(&content_str)?,
        Some("application/javascript") | Some("text/javascript") => {
            crate::beautify::minify_javascript(&content_str)?
        }
        _ => return Ok(content.to_vec()),
    };
    Ok(minified.into_bytes())
}

/// The textual minifier of the legacy format, kept so that recordings made
/// before the current format replay exactly as they did.
pub fn legacy_minify_content(content: &[u8], mime_type: &Option<String>) -> Result<Vec<u8>> {
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
            // Simple CSS minification - preserve @charset at the beginning
            let mut lines: Vec<&str> = content_str
                .lines()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .collect();

            // Extract @charset if it exists (must be first line per CSS spec)
            let charset_line = if !lines.is_empty() && lines[0].starts_with("@charset") {
                Some(lines.remove(0))
            } else {
                None
            };

            // Minify the rest (join without separators)
            let minified_body = lines.join("");

            // Prepend @charset if it existed (must be first, separated by newline)
            if let Some(charset) = charset_line {
                format!("{}\n{}", charset, minified_body)
            } else {
                minified_body
            }
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
        ContentEncodingType::Zstd => {
            let compressed = zstd::stream::encode_all(std::io::Cursor::new(content), 0)?;
            Ok(compressed)
        }
        _ => Ok(content.to_vec()),
    }
}

pub fn re_encode_to_charset(content: &[u8], charset_name: &str) -> Result<Vec<u8>> {
    // File content is stored as UTF-8, convert it back to original charset
    // Use from_utf8_lossy to handle any invalid UTF-8 sequences that may have been introduced
    // during beautification (e.g., from binary data or encoding issues)
    let utf8_str = String::from_utf8_lossy(content);

    // Get the target encoding
    let encoding = Encoding::for_label(charset_name.as_bytes())
        .ok_or_else(|| anyhow::anyhow!("Unknown charset: {}", charset_name))?;

    // If target is UTF-8, no conversion needed
    if encoding == UTF_8 {
        return Ok(content.to_vec());
    }

    // Encode from UTF-8 to target charset
    let (encoded, _encoding_used, _had_errors) = encoding.encode(&utf8_str);
    Ok(encoded.into_owned())
}
