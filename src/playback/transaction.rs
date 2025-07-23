use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use crate::types::{Inventory, Resource, Transaction, BodyChunk};
use crate::traits::FileSystem;

const CHUNK_SIZE: usize = 1024 * 8; // 8KB chunks
const TARGET_MBPS: f64 = 1.0; // Default target speed in Mbps

pub async fn convert_resources_to_transactions<F: FileSystem>(
    inventory: &Inventory,
    inventory_dir: &PathBuf,
    file_system: Arc<F>,
) -> Result<Vec<Transaction>> {
    let mut transactions = Vec::new();
    
    for resource in &inventory.resources {
        if let Some(transaction) = convert_resource_to_transaction(resource, inventory_dir, file_system.clone()).await? {
            transactions.push(transaction);
        }
    }
    
    Ok(transactions)
}

pub async fn convert_resource_to_transaction<F: FileSystem>(
    resource: &Resource,
    inventory_dir: &PathBuf,
    file_system: Arc<F>,
) -> Result<Option<Transaction>> {
    // Load content
    let content = if let Some(file_path) = &resource.content_file_path {
        let full_path = inventory_dir.join("contents").join(file_path);
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
    let processed_content = if resource.minify.unwrap_or(false) {
        minify_content(&content, &resource.content_type_mime)?
    } else {
        content
    };

    // Compress content if needed
    let final_content = if let Some(encoding) = &resource.content_encoding {
        compress_content(&processed_content, encoding)?
    } else {
        processed_content
    };

    // Create chunks
    let chunks = create_chunks(&final_content, resource)?;

    let mut headers = resource.raw_headers.clone().unwrap_or_default();
    
    // Update content-length
    headers.insert("content-length".to_string(), final_content.len().to_string());
    
    // Update charset if it's a text resource
    if let Some(mime_type) = &resource.content_type_mime {
        if let Some(charset) = &resource.content_type_charset {
            headers.insert("content-type".to_string(), format!("{}; charset={}", mime_type, charset));
        } else {
            headers.insert("content-type".to_string(), mime_type.clone());
        }
    }

    Ok(Some(Transaction {
        method: resource.method.clone(),
        url: resource.url.clone(),
        ttfb: resource.ttfb_ms,
        status_code: resource.status_code,
        error_message: resource.error_message.clone(),
        raw_headers: Some(headers),
        chunks,
    }))
}

pub fn create_chunks(content: &[u8], resource: &Resource) -> Result<Vec<BodyChunk>> {
    let mut chunks = Vec::new();
    let total_size = content.len();
    
    if total_size == 0 {
        return Ok(chunks);
    }

    // Calculate timing based on mbps or use default
    let mbps = resource.mbps.unwrap_or(TARGET_MBPS);
    let bytes_per_ms = (mbps * 1024.0 * 1024.0) / 1000.0; // Bytes per millisecond
    
    let mut offset = 0;
    let mut current_time = resource.ttfb_ms;
    
    while offset < total_size {
        let chunk_size = std::cmp::min(CHUNK_SIZE, total_size - offset);
        let chunk_data = content[offset..offset + chunk_size].to_vec();
        
        chunks.push(BodyChunk {
            chunk: chunk_data,
            target_time: current_time,
        });
        
        // Calculate time for next chunk
        let chunk_duration_ms = (chunk_size as f64 / bytes_per_ms) as u64;
        current_time += chunk_duration_ms;
        offset += chunk_size;
    }
    
    Ok(chunks)
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
        _ => content_str.to_string()
    };
    
    Ok(minified.into_bytes())
}

use crate::types::ContentEncodingType;

pub fn compress_content(content: &[u8], encoding: &ContentEncodingType) -> Result<Vec<u8>> {
    match encoding {
        ContentEncodingType::Gzip => {
            use flate2::write::GzEncoder;
            use flate2::Compression;
            use std::io::Write;
            
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(content)?;
            Ok(encoder.finish()?)
        }
        ContentEncodingType::Deflate => {
            use flate2::write::DeflateEncoder;
            use flate2::Compression;
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

