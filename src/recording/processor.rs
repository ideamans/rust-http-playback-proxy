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
                self.process_text_resource(resource, &decompressed_body).await?;
            } else {
                self.process_binary_resource(resource, &decompressed_body).await?;
            }
        } else {
            self.process_binary_resource(resource, &decompressed_body).await?;
        }

        // Calculate mbps
        let body_size = decompressed_body.len() as f64;
        if resource.ttfb_ms > 0 {
            let seconds = (resource.ttfb_ms as f64) / 1000.0;
            resource.mbps = Some((body_size / seconds) / (1024.0 * 1024.0));
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
        // Convert to UTF-8
        let (utf8_content, _detected_encoding) = self.convert_to_utf8(body, &resource.content_type_charset);
        
        // Update charset to UTF-8
        resource.content_type_charset = Some("UTF-8".to_string());
        
        // Check if content was minified by beautifying and comparing line counts
        let original_lines = utf8_content.lines().count();
        let beautified = self.beautify_content(&utf8_content, &resource.content_type_mime)?;
        let beautified_lines = beautified.lines().count();
        
        resource.minify = Some(beautified_lines >= original_lines * 2);
        
        // Save content to file
        let file_path = generate_file_path_from_url(&resource.url, &resource.method)?;
        let full_path = self.contents_dir.join(&file_path);

        if let Some(parent) = full_path.parent() {
            self.file_system.create_dir_all(parent).await?;
        }

        self.file_system.write(&full_path, utf8_content.as_bytes()).await?;
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
                // Simple HTML beautification - add newlines after tags
                let mut result = String::new();
                let mut _in_tag = false;
                
                for ch in content.chars() {
                    result.push(ch);
                    match ch {
                        '<' => _in_tag = true,
                        '>' => {
                            _in_tag = false;
                            result.push('\n');
                        }
                        _ => {}
                    }
                }
                Ok(result)
            }
            Some("text/css") => {
                // Simple CSS beautification
                let result = content
                    .replace('{', "{\n")
                    .replace('}', "\n}\n")
                    .replace(';', ";\n");
                Ok(result)
            }
            Some("application/javascript") | Some("text/javascript") => {
                // Simple JS beautification
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

