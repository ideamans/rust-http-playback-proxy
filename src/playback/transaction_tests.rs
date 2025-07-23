#[cfg(test)]
mod tests {
    use crate::playback::transaction::*;
    use crate::traits::mocks::MockFileSystem;
    use crate::types::{Inventory, Resource, ContentEncodingType};
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_convert_resources_to_transactions_with_file() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();
        
        let mock_fs = Arc::new(MockFileSystem::new());
        
        // Set up mock file content
        let test_content = b"Hello, World!";
        let content_path = inventory_dir.join("contents/get/https/example.com/test.txt");
        mock_fs.set_file(&content_path.to_string_lossy(), test_content.to_vec());
        
        let mut inventory = Inventory::new();
        let mut resource = Resource::new("GET".to_string(), "https://example.com/test.txt".to_string());
        resource.content_file_path = Some("get/https/example.com/test.txt".to_string());
        resource.status_code = Some(200);
        resource.ttfb_ms = 100;
        resource.mbps = Some(2.0);
        
        inventory.resources.push(resource);
        
        let transactions = convert_resources_to_transactions(&inventory, &inventory_dir, mock_fs)
            .await
            .unwrap();
        
        assert_eq!(transactions.len(), 1);
        let transaction = &transactions[0];
        assert_eq!(transaction.method, "GET");
        assert_eq!(transaction.url, "https://example.com/test.txt");
        assert_eq!(transaction.ttfb, 100);
        assert_eq!(transaction.status_code, Some(200));
        assert!(!transaction.chunks.is_empty());
        
        // Verify chunks contain the original content
        let mut combined = Vec::new();
        for chunk in &transaction.chunks {
            combined.extend_from_slice(&chunk.chunk);
        }
        assert_eq!(combined, test_content);
    }

    #[tokio::test]
    async fn test_convert_resources_to_transactions_with_utf8() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();
        
        let mock_fs = Arc::new(MockFileSystem::new());
        
        let mut inventory = Inventory::new();
        let mut resource = Resource::new("GET".to_string(), "https://example.com/api/data".to_string());
        resource.content_utf8 = Some("{ \"message\": \"Hello, World!\" }".to_string());
        resource.status_code = Some(200);
        resource.ttfb_ms = 50;
        
        inventory.resources.push(resource);
        
        let transactions = convert_resources_to_transactions(&inventory, &inventory_dir, mock_fs)
            .await
            .unwrap();
        
        assert_eq!(transactions.len(), 1);
        let transaction = &transactions[0];
        
        // Verify chunks contain the UTF-8 content
        let mut combined = Vec::new();
        for chunk in &transaction.chunks {
            combined.extend_from_slice(&chunk.chunk);
        }
        let combined_string = String::from_utf8(combined).unwrap();
        assert_eq!(combined_string, "{ \"message\": \"Hello, World!\" }");
    }

    #[tokio::test]
    async fn test_convert_resource_with_base64() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();
        
        let mock_fs = Arc::new(MockFileSystem::new());
        
        use base64::{Engine as _, engine::general_purpose};
        let original_content = b"Binary data content";
        let base64_content = general_purpose::STANDARD.encode(original_content);
        
        let mut resource = Resource::new("GET".to_string(), "https://example.com/image.png".to_string());
        resource.content_base64 = Some(base64_content);
        resource.status_code = Some(200);
        resource.ttfb_ms = 200;
        
        let transaction = convert_resource_to_transaction(&resource, &inventory_dir, mock_fs)
            .await
            .unwrap();
        
        assert!(transaction.is_some());
        let transaction = transaction.unwrap();
        
        // Verify chunks contain the decoded content
        let mut combined = Vec::new();
        for chunk in &transaction.chunks {
            combined.extend_from_slice(&chunk.chunk);
        }
        assert_eq!(combined, original_content);
    }

    #[test]
    fn test_create_chunks_timing() {
        let mut resource = Resource::new("GET".to_string(), "https://example.com/large-file".to_string());
        resource.ttfb_ms = 100;
        resource.mbps = Some(1.0); // 1 Mbps = 1024*1024 bits/sec = 128 KB/s
        
        let content = vec![0u8; 1024]; // 1KB content
        let chunks = create_chunks(&content, &resource).unwrap();
        
        assert!(!chunks.is_empty());
        
        // First chunk should start at ttfb
        assert_eq!(chunks[0].target_time, 100);
        
        // Each subsequent chunk should have a later target_time
        for i in 1..chunks.len() {
            assert!(chunks[i].target_time > chunks[i-1].target_time);
        }
    }

    #[test]
    fn test_minify_html_content() {
        let html_with_whitespace = b"<html>\n  <head>\n    <title>Test</title>\n  </head>\n  <body>\n    <h1>Hello</h1>\n  </body>\n</html>";
        
        let minified = minify_content(html_with_whitespace, &Some("text/html".to_string())).unwrap();
        let minified_str = String::from_utf8(minified).unwrap();
        
        // Should be more compact
        assert!(minified_str.len() <= html_with_whitespace.len());
        // Should not contain excessive whitespace
        assert!(!minified_str.contains("  "));
    }

    #[test]
    fn test_minify_css_content() {
        let css_with_whitespace = b"body {\n  margin: 0;\n  padding: 0;\n}\n\nh1 {\n  color: red;\n}";
        
        let minified = minify_content(css_with_whitespace, &Some("text/css".to_string())).unwrap();
        let minified_str = String::from_utf8(minified).unwrap();
        
        // Should be more compact
        assert!(minified_str.len() <= css_with_whitespace.len());
        // Should not contain newlines (basic minification)
        assert!(!minified_str.contains('\n'));
    }

    #[test]
    fn test_compress_gzip_content() {
        let content = b"This is test content for compression testing. It should be compressed efficiently. This is a longer text to ensure compression works properly and reduces the size significantly.";
        
        let compressed = compress_content(content, &ContentEncodingType::Gzip).unwrap();
        
        // Compressed content should be different from original
        assert_ne!(compressed, content);
        // For longer content, gzip should compress it
        assert!(compressed.len() < content.len());
    }

    #[test]
    fn test_compress_identity_content() {
        let content = b"This content should not be compressed.";
        
        let result = compress_content(content, &ContentEncodingType::Identity).unwrap();
        
        // Identity compression should return the same content
        assert_eq!(result, content);
    }

    #[test]
    fn test_empty_content_chunks() {
        let resource = Resource::new("GET".to_string(), "https://example.com/empty".to_string());
        let empty_content = b"";
        
        let chunks = create_chunks(empty_content, &resource).unwrap();
        
        // Empty content should result in empty chunks
        assert!(chunks.is_empty());
    }

    #[tokio::test]
    async fn test_convert_resource_no_content() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();
        
        let mock_fs = Arc::new(MockFileSystem::new());
        
        // Resource with no content
        let resource = Resource::new("GET".to_string(), "https://example.com/empty".to_string());
        
        let result = convert_resource_to_transaction(&resource, &inventory_dir, mock_fs)
            .await
            .unwrap();
        
        // Should return None for resources with no content
        assert!(result.is_none());
    }

    #[test] 
    fn test_chunk_target_times() {
        let mut resource = Resource::new("GET".to_string(), "https://example.com/test".to_string());
        resource.ttfb_ms = 50;
        resource.mbps = Some(2.0); // 2 Mbps
        
        let content = vec![0u8; 2048]; // 2KB content
        let chunks = create_chunks(&content, &resource).unwrap();
        
        // All target times should be >= ttfb
        for chunk in &chunks {
            assert!(chunk.target_time >= resource.ttfb_ms);
        }
        
        // Target times should be increasing
        for i in 1..chunks.len() {
            assert!(chunks[i].target_time >= chunks[i-1].target_time);
        }
    }
}