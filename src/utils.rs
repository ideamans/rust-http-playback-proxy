use anyhow::Result;
use sha1::{Digest, Sha1};
use std::net::TcpListener;
use url::Url;

pub fn find_available_port(start_port: u16) -> Result<u16> {
    for port in start_port..=65535 {
        if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{}", port)) {
            drop(listener);
            return Ok(port);
        }
    }
    anyhow::bail!("No available port found starting from {}", start_port)
}

pub fn get_port_or_default(port: Option<u16>) -> Result<u16> {
    match port {
        Some(p) => Ok(p),
        None => find_available_port(18080),
    }
}

#[allow(dead_code)]
pub fn generate_file_path_from_url(url: &str, method: &str) -> Result<String> {
    let parsed_url = Url::parse(url)?;
    let scheme = parsed_url.scheme();
    let host = parsed_url.host_str().unwrap_or("localhost");
    let path = parsed_url.path();

    let mut file_path = format!("{}/{}/{}", method.to_lowercase(), scheme, host);

    // Handle path
    let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if path_segments.is_empty() {
        file_path.push_str("/index.html");
    } else {
        for segment in &path_segments {
            file_path.push('/');
            file_path.push_str(segment);
        }

        // If path ends with '/', add index.html
        if path.ends_with('/') {
            file_path.push_str("/index.html");
        }

        // Add query parameters to filename
        #[allow(clippy::collapsible_if)]
        if let Some(query) = parsed_url.query() {
            if !query.is_empty() {
                if query.len() <= 32 {
                    // Simple case: add query as is
                    let encoded_query = urlencoding::encode(query);
                    if let Some(last_segment) = file_path.split('/').next_back() {
                        if let Some(dot_pos) = last_segment.rfind('.') {
                            let (name, ext) = last_segment.split_at(dot_pos);
                            let new_file_path =
                                file_path[..file_path.len() - last_segment.len()].to_string();
                            file_path =
                                format!("{}{}~{}{}", new_file_path, name, encoded_query, ext);
                        } else {
                            file_path.push_str(&format!("~{}", encoded_query));
                        }
                    }
                } else {
                    // Long query: use first 32 chars + hash
                    let first_32 = &query[..32];
                    let remaining = &query[32..];
                    let mut hasher = Sha1::new();
                    hasher.update(remaining.as_bytes());
                    let hash = hex::encode(hasher.finalize());

                    let encoded_first_32 = urlencoding::encode(first_32);
                    if let Some(last_segment) = file_path.split('/').next_back() {
                        if let Some(dot_pos) = last_segment.rfind('.') {
                            let (name, ext) = last_segment.split_at(dot_pos);
                            let new_file_path =
                                file_path[..file_path.len() - last_segment.len()].to_string();
                            file_path = format!(
                                "{}{}~{}.~{}{}",
                                new_file_path, name, encoded_first_32, hash, ext
                            );
                        } else {
                            file_path.push_str(&format!("~{}.~{}", encoded_first_32, hash));
                        }
                    }
                }
            }
        }
    }

    Ok(file_path)
}

#[allow(dead_code)]
pub fn is_text_resource(content_type: &str) -> bool {
    let content_type = content_type.to_lowercase();
    content_type.starts_with("text/html")
        || content_type.starts_with("text/css")
        || content_type.starts_with("application/javascript")
        || content_type.starts_with("text/javascript")
}

#[allow(dead_code)]
pub fn extract_charset_from_content_type(content_type: &str) -> Option<String> {
    if let Some(charset_pos) = content_type.to_lowercase().find("charset=") {
        let charset_start = charset_pos + 8;
        let charset_part = &content_type[charset_start..];
        let charset = if let Some(semicolon_pos) = charset_part.find(';') {
            &charset_part[..semicolon_pos]
        } else {
            charset_part
        };
        Some(charset.trim().trim_matches('"').to_string())
    } else {
        None
    }
}

/// Extract charset from HTML content (looks for <meta charset> or <meta http-equiv>)
/// This function searches the first 8KB of content to avoid processing large files
#[allow(dead_code)]
pub fn extract_charset_from_html(content: &[u8]) -> Option<String> {
    // Only search first 8KB (head section should be near the start)
    let search_len = content.len().min(8192);
    let search_content = &content[..search_len];

    // Convert to lowercase ASCII for case-insensitive search
    let content_lower = String::from_utf8_lossy(search_content).to_lowercase();

    // Pattern 1: <meta charset="xxx">
    if let Some(pos) = content_lower.find("<meta charset=") {
        let after_equals = &content_lower[pos + 14..];
        let charset = if let Some(stripped) = after_equals.strip_prefix('"') {
            // charset="xxx"
            stripped
                .find('"')
                .map(|end_quote| stripped[..end_quote].to_string())
        } else if let Some(stripped) = after_equals.strip_prefix('\'') {
            // charset='xxx'
            stripped
                .find('\'')
                .map(|end_quote| stripped[..end_quote].to_string())
        } else {
            // charset=xxx (no quotes)
            let end = after_equals
                .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                .unwrap_or(after_equals.len());
            Some(after_equals[..end].to_string())
        };

        if charset.is_some() {
            return charset;
        }
    }

    // Pattern 2: <meta http-equiv="Content-Type" content="text/html; charset=xxx">
    if let Some(pos) = content_lower.find("http-equiv") {
        let after_equiv = &content_lower[pos..];
        if let Some(content_pos) = after_equiv.find("content=") {
            let after_content = &after_equiv[content_pos + 8..];
            // Extract the content attribute value
            let content_value = if let Some(stripped) = after_content.strip_prefix('"') {
                if let Some(end_quote) = stripped.find('"') {
                    &stripped[..end_quote]
                } else {
                    ""
                }
            } else if let Some(stripped) = after_content.strip_prefix('\'') {
                if let Some(end_quote) = stripped.find('\'') {
                    &stripped[..end_quote]
                } else {
                    ""
                }
            } else {
                ""
            };

            // Now extract charset from the content attribute
            return extract_charset_from_content_type(content_value);
        }
    }

    None
}

/// Extract charset from CSS content (looks for @charset "xxx";)
/// This function searches the first 1KB of content as @charset must appear at the start
#[allow(dead_code)]
pub fn extract_charset_from_css(content: &[u8]) -> Option<String> {
    // @charset must be the first thing in the file (ignoring whitespace/comments)
    // Only search first 1KB
    let search_len = content.len().min(1024);
    let search_content = &content[..search_len];

    // Convert to lowercase ASCII for case-insensitive search
    let content_lower = String::from_utf8_lossy(search_content).to_lowercase();

    // Look for @charset "xxx"; or @charset 'xxx';
    if let Some(pos) = content_lower.find("@charset") {
        let after_charset = &content_lower[pos + 8..].trim_start();

        let charset = if let Some(stripped) = after_charset.strip_prefix('"') {
            // @charset "xxx";
            stripped
                .find('"')
                .map(|end_quote| stripped[..end_quote].to_string())
        } else if let Some(stripped) = after_charset.strip_prefix('\'') {
            // @charset 'xxx';
            stripped
                .find('\'')
                .map(|end_quote| stripped[..end_quote].to_string())
        } else {
            None
        };

        return charset;
    }

    None
}

mod tests;
