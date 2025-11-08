use anyhow::Result;
use std::path::PathBuf;
use flate2::read::GzDecoder;
use std::io::Read;
use encoding_rs::{Encoding, UTF_8};
use std::sync::Arc;
use crate::types::{Resource, ContentEncodingType};
use crate::utils::{is_text_resource, extract_charset_from_content_type, generate_file_path_from_url};
use crate::traits::{FileSystem, TimeProvider};

#[allow(dead_code)]
pub struct RequestProcessor<F: FileSystem, T: TimeProvider> {
    inventory_dir: PathBuf,
    contents_dir: PathBuf,
    file_system: Arc<F>,
    time_provider: Arc<T>,
}

impl<F: FileSystem, T: TimeProvider> RequestProcessor<F, T> {
    pub fn new(inventory_dir: PathBuf, file_system: Arc<F>, time_provider: Arc<T>) -> Self {
        let contents_dir = inventory_dir.join("contents");
        Self {
            inventory_dir,
            contents_dir,
            file_system,
            time_provider,
        }
    }

    #[allow(dead_code)]
    pub async fn process_response_body(
        &self,
        resource: &mut Resource,
        body: &[u8],
        content_type: Option<&str>,
    ) -> Result<()> {
        let decompressed_body = self.decompress_body(body, &resource.content_encoding)?;
        
        if let Some(ct) = content_type {
            resource.content_type_mime = Some(ct.split(';').next().unwrap_or(ct).trim().to_string());
            resource.content_type_charset = extract_charset_from_content_type(ct);

            if is_text_resource(ct) {
                // Try to process as text, fallback to binary if it fails
                if let Err(e) = self.process_text_resource(resource, &decompressed_body).await {
                    tracing::warn!("Failed to process as text resource, falling back to binary: {}", e);
                    self.process_binary_resource(resource, &decompressed_body).await?;
                }
            } else {
                self.process_binary_resource(resource, &decompressed_body).await?;
            }
        } else {
            self.process_binary_resource(resource, &decompressed_body).await?;
        }

        // Calculate mbps (megabits per second) from compressed body size
        // This is the actual network transfer speed, excluding latency/TTFB
        // where download_time = download_end_ms - ttfb_ms
        let body_size = body.len() as f64; // Use compressed body size (what was actually transferred)
        if let Some(download_end_ms) = resource.download_end_ms {
            if download_end_ms > resource.ttfb_ms {
                let download_time_ms = download_end_ms - resource.ttfb_ms;
                let seconds = (download_time_ms as f64) / 1000.0;
                if seconds > 0.0 {
                    // bytes/s -> bits/s -> Mb/s (megabits per second)
                    let bytes_per_second = body_size / seconds;
                    let bits_per_second = bytes_per_second * 8.0;
                    resource.mbps = Some(bits_per_second / (1000.0 * 1000.0));
                }
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn decompress_body(
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
            _ => Ok(body.to_vec()),
        }
    }

    #[allow(dead_code)]
    pub async fn process_text_resource(
        &self,
        resource: &mut Resource,
        body: &[u8],
    ) -> Result<()> {
        // Save original charset before conversion
        resource.original_charset = resource.content_type_charset.clone();

        // Convert to UTF-8
        let (utf8_content, _detected_encoding) = self.convert_to_utf8(body, &resource.content_type_charset);

        // Update charset to UTF-8 (for internal storage only)
        resource.content_type_charset = Some("UTF-8".to_string());

        // Check if content was minified by beautifying and comparing line counts
        let original_lines = utf8_content.lines().count();
        let beautified = self.beautify_content(&utf8_content, &resource.content_type_mime)?;
        let beautified_lines = beautified.lines().count();

        let is_minified = beautified_lines >= original_lines * 2;
        resource.minify = Some(is_minified);

        // Save content to file
        // If minified, save the beautified version for better editability
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

        self.file_system.write(&full_path, content_to_save.as_bytes()).await?;
        // Store path relative to inventory dir (with "contents/" prefix)
        resource.content_file_path = Some(format!("contents/{}", file_path));

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn process_binary_resource(
        &self,
        resource: &mut Resource,
        body: &[u8],
    ) -> Result<()> {
        // Save binary content as base64
        use base64::{Engine as _, engine::general_purpose};
        resource.content_base64 = Some(general_purpose::STANDARD.encode(body));
        
        // Also save to file
        let file_path = generate_file_path_from_url(&resource.url, &resource.method)?;
        let full_path = self.contents_dir.join(&file_path);

        if let Some(parent) = full_path.parent() {
            self.file_system.create_dir_all(parent).await?;
        }

        self.file_system.write(&full_path, body).await?;
        // Store path relative to inventory dir (with "contents/" prefix)
        resource.content_file_path = Some(format!("contents/{}", file_path));
        
        Ok(())
    }

    #[allow(dead_code)]
    pub fn convert_to_utf8(&self, body: &[u8], charset: &Option<String>) -> (String, &'static str) {
        let encoding = if let Some(charset_name) = charset {
            Encoding::for_label(charset_name.as_bytes()).unwrap_or(UTF_8)
        } else {
            UTF_8
        };

        let (cow, encoding_used, _had_errors) = encoding.decode(body);
        (cow.into_owned(), encoding_used.name())
    }


    #[allow(dead_code)]
    pub fn beautify_content(&self, content: &str, mime_type: &Option<String>) -> Result<String> {
        match mime_type.as_deref() {
            Some("text/html") => {
                // Use prettyish-html library
                Ok(prettyish_html::prettify(content).to_string())
            }
            Some("application/javascript") | Some("text/javascript") => {
                // Use prettify-js library
                let (prettified, _source_map) = prettify_js::prettyprint(content);
                Ok(prettified)
            }
            Some("text/css") => {
                // Simple CSS beautification (no library available for current Rust version)
                let result = content
                    .replace('{', "{\n")
                    .replace('}', "\n}\n")
                    .replace(';', ";\n");
                Ok(result)
            }
            _ => Ok(content.to_string())
        }
    }
}

