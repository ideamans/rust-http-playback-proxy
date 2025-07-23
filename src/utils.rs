use anyhow::Result;
use std::net::TcpListener;
use url::Url;
use sha1::{Digest, Sha1};
use hex;

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
        None => find_available_port(8080),
    }
}

pub fn generate_file_path_from_url(url: &str, method: &str) -> Result<String> {
    let parsed_url = Url::parse(url)?;
    let scheme = parsed_url.scheme();
    let path = parsed_url.path();
    
    let mut file_path = format!("{}/{}", method.to_lowercase(), scheme);
    
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
        if let Some(query) = parsed_url.query() {
            if !query.is_empty() {
                if query.len() <= 32 {
                    // Simple case: add query as is
                    let encoded_query = urlencoding::encode(query);
                    if let Some(last_segment) = file_path.split('/').last() {
                        if let Some(dot_pos) = last_segment.rfind('.') {
                            let (name, ext) = last_segment.split_at(dot_pos);
                            let new_file_path = file_path[..file_path.len() - last_segment.len()].to_string();
                            file_path = format!("{}{}~{}{}", new_file_path, name, encoded_query, ext);
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
                    if let Some(last_segment) = file_path.split('/').last() {
                        if let Some(dot_pos) = last_segment.rfind('.') {
                            let (name, ext) = last_segment.split_at(dot_pos);
                            let new_file_path = file_path[..file_path.len() - last_segment.len()].to_string();
                            file_path = format!("{}{}~{}.~{}{}", new_file_path, name, encoded_first_32, hash, ext);
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

pub fn is_text_resource(content_type: &str) -> bool {
    let content_type = content_type.to_lowercase();
    content_type.starts_with("text/html") ||
    content_type.starts_with("text/css") ||
    content_type.starts_with("application/javascript") ||
    content_type.starts_with("text/javascript")
}

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

mod tests;