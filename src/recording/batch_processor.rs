use crate::traits::{FileSystem, TimeProvider};
use crate::types::{ContentEncodingType, Inventory, OriginalBody, Resource};
use crate::utils::{
    extract_charset_from_content_type, extract_charset_from_css, extract_charset_from_html,
    generate_file_path_from_url, is_text_resource,
};
use anyhow::Result;
use encoding_rs::{Encoding, UTF_8};
use flate2::read::GzDecoder;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info};

/// Batch processor for processing all resources at shutdown time
/// This allows us to keep proxy runtime overhead minimal for accurate timing
pub struct BatchProcessor<F: FileSystem, T: TimeProvider> {
    contents_dir: PathBuf,
    originals_dir: PathBuf,
    file_system: Arc<F>,
    _time_provider: Arc<T>,
}

impl<F: FileSystem, T: TimeProvider> BatchProcessor<F, T> {
    pub fn new(inventory_dir: PathBuf, file_system: Arc<F>, time_provider: Arc<T>) -> Self {
        let contents_dir = inventory_dir.join("contents");
        let originals_dir = inventory_dir.join("originals");
        Self {
            contents_dir,
            originals_dir,
            file_system,
            _time_provider: time_provider,
        }
    }

    /// Process all resources in the inventory at shutdown time
    /// This includes:
    /// - Decompressing response bodies
    /// - Converting to UTF-8 for text resources
    /// - Detecting and extracting charset information
    /// - Beautifying minified content
    /// - Saving to content files
    pub async fn process_all(&self, inventory: &mut Inventory) -> Result<()> {
        // Everything written below follows the current format: precise mbps,
        // originals for unedited replay, HTML kept as recorded.
        inventory.format_version = Some(crate::types::CURRENT_FORMAT_VERSION);
        info!(
            "Batch processing {} resources...",
            inventory.resources.len()
        );

        let total = inventory.resources.len();
        for (idx, resource) in inventory.resources.iter_mut().enumerate() {
            info!("Processing resource {}/{}...", idx + 1, total);
            if let Err(e) = self.process_resource(resource).await {
                error!(
                    "Failed to process resource #{} ({}): {}",
                    idx + 1,
                    resource.url,
                    e
                );
            }
        }

        info!("Batch processing completed");
        Ok(())
    }

    async fn process_resource(&self, resource: &mut Resource) -> Result<()> {
        // Extract raw body (if present)
        let raw_body = match &resource.raw_body {
            Some(body) => body.clone(),
            None => {
                return Ok(()); // No body to process
            }
        };

        // Decompress body
        let decompressed_body = self.decompress_body(&raw_body, &resource.content_encoding)?;

        // Determine if this is a text resource
        let is_text = resource
            .content_type_mime
            .as_deref()
            .map(is_text_resource)
            .unwrap_or(false);

        let saved = if is_text {
            // Extract and save charset from Content-Type for text resources
            if let Some(content_type_header) = resource
                .raw_headers
                .as_ref()
                .and_then(|h| h.get("content-type"))
            {
                let ct_str = content_type_header.as_vec().join("; ");
                let mut charset = extract_charset_from_content_type(&ct_str);

                // If HTTP header doesn't have charset, try to detect from content
                if charset.is_none() {
                    let mime = resource.content_type_mime.as_deref().unwrap_or("");
                    charset = if mime == "text/html" {
                        extract_charset_from_html(&decompressed_body)
                    } else if mime == "text/css" {
                        extract_charset_from_css(&decompressed_body)
                    } else {
                        None
                    };
                }

                resource.content_charset = charset;
            }

            // Try to process as text, fallback to binary if it fails
            match self
                .process_text_resource(resource, &decompressed_body)
                .await
            {
                Ok(saved) => saved,
                Err(e) => {
                    tracing::warn!(
                        "Failed to process as text resource ({}), falling back to binary: {}",
                        resource.url,
                        e
                    );
                    self.process_binary_resource(resource, &decompressed_body)
                        .await?
                }
            }
        } else {
            self.process_binary_resource(resource, &decompressed_body)
                .await?
        };

        self.save_original(resource, &raw_body, &saved).await?;

        // Clear raw_body to free memory after processing
        resource.raw_body = None;

        Ok(())
    }

    /// Keep the body exactly as it crossed the wire whenever the editable file
    /// differs from it (decompressed, converted to UTF-8 or reformatted), together
    /// with what playback needs to tell whether the resource has been edited since.
    async fn save_original(
        &self,
        resource: &mut Resource,
        raw_body: &[u8],
        saved: &[u8],
    ) -> Result<()> {
        if raw_body == saved {
            // The editable file is the wire body; serving it is already exact.
            return Ok(());
        }
        let Some(content_path) = resource.content_file_path.as_deref() else {
            return Ok(());
        };
        let relative = content_path
            .strip_prefix("contents/")
            .unwrap_or(content_path);
        let full_path = self.originals_dir.join(relative);
        if let Some(parent) = full_path.parent() {
            self.file_system.create_dir_all(parent).await?;
        }
        self.file_system.write(&full_path, raw_body).await?;

        resource.original = Some(OriginalBody {
            file_path: format!("originals/{}", relative),
            content_sha256: crate::utils::sha256_hex(saved),
            content_encoding: resource.content_encoding.clone(),
            content_charset: resource.content_charset.clone(),
            minify: resource.minify,
        });
        Ok(())
    }

    fn decompress_body(
        &self,
        body: &[u8],
        encoding: &Option<ContentEncodingType>,
    ) -> Result<Vec<u8>> {
        match encoding {
            Some(ContentEncodingType::Gzip) => {
                let mut decoder = GzDecoder::new(body);
                let mut decompressed = Vec::new();
                decoder.read_to_end(&mut decompressed)?;
                Ok(decompressed)
            }
            Some(ContentEncodingType::Deflate) => {
                let mut decompressed = Vec::new();
                let mut decoder = flate2::read::DeflateDecoder::new(body);
                decoder.read_to_end(&mut decompressed)?;
                Ok(decompressed)
            }
            Some(ContentEncodingType::Br) => {
                let mut decompressed = Vec::new();
                brotli::BrotliDecompress(&mut std::io::Cursor::new(body), &mut decompressed)?;
                Ok(decompressed)
            }
            Some(ContentEncodingType::Zstd) => {
                let decompressed = zstd::stream::decode_all(std::io::Cursor::new(body))?;
                Ok(decompressed)
            }
            _ => Ok(body.to_vec()),
        }
    }

    async fn process_text_resource(&self, resource: &mut Resource, body: &[u8]) -> Result<Vec<u8>> {
        // Convert to UTF-8 (content_charset already saved in process_resource)
        let (utf8_content, _detected_encoding) =
            self.convert_to_utf8(body, &resource.content_charset);

        // Check if content was minified by beautifying and comparing line counts
        // If beautification fails, log warning and save original UTF-8 content
        let original_lines = utf8_content.lines().count();
        let (beautified, is_minified) =
            match self.beautify_content(&utf8_content, &resource.content_type_mime) {
                Ok(beautified) => {
                    let beautified_lines = beautified.lines().count();
                    let is_minified = beautified_lines >= original_lines * 2;
                    (beautified, is_minified)
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to beautify content for {}: {}. Saving original UTF-8 content.",
                        resource.url,
                        e
                    );
                    // Use original content, mark as not minified
                    (utf8_content.clone(), false)
                }
            };

        resource.minify = Some(is_minified);

        // Save content to file
        // If minified, save the beautified version for better editability
        // NOTE: Content is saved as UTF-8, but charset declarations (like <meta charset="Shift_JIS">)
        // are PRESERVED in the file. During playback, the content will be re-encoded to the original
        // charset based on resource.content_charset metadata.
        let content_to_save = if is_minified {
            beautified
        } else {
            utf8_content
        };

        let file_path = generate_file_path_from_url(&resource.url, &resource.method)?;
        let full_path = self.contents_dir.join(&file_path);

        if let Some(parent) = full_path.parent() {
            self.file_system.create_dir_all(parent).await?;
        }

        self.file_system
            .write(&full_path, content_to_save.as_bytes())
            .await?;
        // Store path relative to inventory dir (with "contents/" prefix)
        resource.content_file_path = Some(format!("contents/{}", file_path));

        Ok(content_to_save.into_bytes())
    }

    async fn process_binary_resource(
        &self,
        resource: &mut Resource,
        body: &[u8],
    ) -> Result<Vec<u8>> {
        // The file is the only copy: an inline base64 duplicate would go stale
        // the moment the file is edited, and bloats index.json.
        let file_path = generate_file_path_from_url(&resource.url, &resource.method)?;
        let full_path = self.contents_dir.join(&file_path);

        if let Some(parent) = full_path.parent() {
            self.file_system.create_dir_all(parent).await?;
        }

        self.file_system.write(&full_path, body).await?;
        // Store path relative to inventory dir (with "contents/" prefix)
        resource.content_file_path = Some(format!("contents/{}", file_path));

        Ok(body.to_vec())
    }

    fn convert_to_utf8(&self, body: &[u8], charset: &Option<String>) -> (String, &'static str) {
        let encoding = if let Some(charset_name) = charset {
            Encoding::for_label(charset_name.as_bytes()).unwrap_or(UTF_8)
        } else {
            UTF_8
        };

        let (cow, encoding_used, _had_errors) = encoding.decode(body);
        (cow.into_owned(), encoding_used.name())
    }

    fn beautify_content(&self, content: &str, mime_type: &Option<String>) -> Result<String> {
        match mime_type.as_deref() {
            // HTML is kept as recorded. Re-serialising the DOM changes whitespace
            // between inline elements, character references and <pre> blocks, so
            // a pretty-printed copy is not the same document.
            Some("application/javascript") | Some("text/javascript") => {
                crate::beautify::format_javascript(content)
            }
            Some("text/css") => crate::beautify::format_css(content),
            _ => Ok(content.to_string()),
        }
    }
}
