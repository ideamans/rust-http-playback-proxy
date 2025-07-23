#[cfg(test)]
mod tests {
    use crate::types::{DeviceType, Inventory, Resource, ContentEncodingType};
    use tempfile::TempDir;
    use tokio;

    #[tokio::test]
    async fn test_load_inventory() {
        use crate::playback::load_inventory;
        
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();
        
        // Create a test inventory file
        let mut inventory = Inventory::new();
        inventory.entry_url = Some("https://example.com".to_string());
        inventory.device_type = Some(DeviceType::Desktop);
        
        let resource = Resource::new("GET".to_string(), "https://example.com/test".to_string());
        inventory.resources.push(resource);
        
        // Save the inventory
        tokio::fs::create_dir_all(&inventory_dir).await.unwrap();
        let inventory_path = inventory_dir.join("inventory.json");
        let inventory_json = serde_json::to_string_pretty(&inventory).unwrap();
        tokio::fs::write(&inventory_path, inventory_json).await.unwrap();
        
        // Test loading
        let loaded_inventory = load_inventory(&inventory_dir).await.unwrap();
        
        assert_eq!(loaded_inventory.entry_url, Some("https://example.com".to_string()));
        assert_eq!(loaded_inventory.device_type, Some(DeviceType::Desktop));
        assert_eq!(loaded_inventory.resources.len(), 1);
    }

    #[tokio::test]
    async fn test_convert_resources_to_transactions() {
        use crate::playback::transaction::convert_resources_to_transactions;
        
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();
        
        let mut inventory = Inventory::new();
        
        // Create a test resource with UTF-8 content
        let mut resource = Resource::new("GET".to_string(), "https://example.com/test".to_string());
        resource.status_code = Some(200);
        resource.ttfb_ms = 100;
        resource.content_utf8 = Some("Hello, World!".to_string());
        resource.mbps = Some(2.0);
        
        inventory.resources.push(resource);
        
        // Convert to transactions
        let transactions = convert_resources_to_transactions(&inventory, &inventory_dir).await.unwrap();
        
        assert_eq!(transactions.len(), 1);
        
        let transaction = &transactions[0];
        assert_eq!(transaction.method, "GET");
        assert_eq!(transaction.url, "https://example.com/test");
        assert_eq!(transaction.ttfb, 100);
        assert_eq!(transaction.status_code, Some(200));
        assert!(!transaction.chunks.is_empty());
    }

    #[tokio::test]
    async fn test_convert_resource_with_file() {
        use crate::playback::transaction::convert_resource_to_transaction;
        
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();
        let contents_dir = inventory_dir.join("contents");
        tokio::fs::create_dir_all(&contents_dir).await.unwrap();
        
        // Create a test file
        let test_content = b"Test file content";
        let file_path = "get/https/example.com/test.txt";
        let full_file_path = contents_dir.join(file_path);
        if let Some(parent) = full_file_path.parent() {
            tokio::fs::create_dir_all(parent).await.unwrap();
        }
        tokio::fs::write(&full_file_path, test_content).await.unwrap();
        
        // Create a resource that references this file
        let mut resource = Resource::new("GET".to_string(), "https://example.com/test.txt".to_string());
        resource.status_code = Some(200);
        resource.ttfb_ms = 50;
        resource.content_file_path = Some(file_path.to_string());
        resource.mbps = Some(1.0);
        
        // Convert to transaction
        let transaction = convert_resource_to_transaction(&resource, &inventory_dir).await.unwrap();
        
        assert!(transaction.is_some());
        let transaction = transaction.unwrap();
        
        assert_eq!(transaction.method, "GET");
        assert_eq!(transaction.url, "https://example.com/test.txt");
        assert_eq!(transaction.ttfb, 50);
        assert_eq!(transaction.status_code, Some(200));
        
        // Check that chunks were created
        assert!(!transaction.chunks.is_empty());
        
        // Verify content by combining chunks
        let mut combined_content = Vec::new();
        for chunk in &transaction.chunks {
            combined_content.extend_from_slice(&chunk.chunk);
        }
        assert_eq!(combined_content, test_content);
    }

    #[test]
    fn test_create_chunks() {
        use crate::playback::transaction::create_chunks;
        
        let mut resource = Resource::new("GET".to_string(), "https://example.com/test".to_string());
        resource.ttfb_ms = 100;
        resource.mbps = Some(1.0); // 1 Mbps
        
        let content = b"This is test content for chunking";
        let chunks = create_chunks(content, &resource).unwrap();
        
        assert!(!chunks.is_empty());
        
        // Verify that all chunks have target times >= ttfb
        for chunk in &chunks {
            assert!(chunk.target_time >= resource.ttfb_ms);
        }
        
        // Verify that combined chunks equal original content
        let mut combined = Vec::new();
        for chunk in &chunks {
            combined.extend_from_slice(&chunk.chunk);
        }
        assert_eq!(combined, content);
    }

    #[test]
    fn test_minify_content() {
        use crate::playback::transaction::minify_content;
        
        // Test HTML minification
        let html_content = b"<html>\n  <body>\n    <h1>Test</h1>\n  </body>\n</html>";
        let minified = minify_content(html_content, &Some("text/html".to_string())).unwrap();
        let minified_str = String::from_utf8(minified).unwrap();
        
        // Should have fewer newlines and spaces
        assert!(minified_str.len() <= html_content.len());
        assert!(!minified_str.contains("  ")); // No double spaces
        
        // Test CSS minification
        let css_content = b"body {\n  margin: 0;\n  padding: 0;\n}";
        let minified = minify_content(css_content, &Some("text/css".to_string())).unwrap();
        let minified_str = String::from_utf8(minified).unwrap();
        
        // Should be more compact
        assert!(minified_str.len() <= css_content.len());
    }

    #[test]
    fn test_compress_content() {
        use crate::playback::transaction::compress_content;
        
        let content = b"This is some test content that should be compressed";
        
        // Test Gzip compression
        let compressed = compress_content(content, &ContentEncodingType::Gzip).unwrap();
        assert!(compressed.len() > 0);
        assert_ne!(compressed, content);
        
        // Test Deflate compression
        let compressed = compress_content(content, &ContentEncodingType::Deflate).unwrap();
        assert!(compressed.len() > 0);
        assert_ne!(compressed, content);
        
        // Test Identity (no compression)
        let not_compressed = compress_content(content, &ContentEncodingType::Identity).unwrap();
        assert_eq!(not_compressed, content);
    }
}