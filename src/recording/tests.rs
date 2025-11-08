#[cfg(test)]
mod recording_tests {
    use crate::types::{DeviceType, Inventory};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_processor_creation() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();

        use crate::traits::{RealFileSystem, RealTimeProvider};
        let processor = crate::recording::processor::RequestProcessor::new(
            inventory_dir.clone(),
            std::sync::Arc::new(RealFileSystem),
            std::sync::Arc::new(RealTimeProvider::new()),
        );

        // The processor should be created successfully
        // (This tests the basic constructor)
        drop(processor);
    }

    #[tokio::test]
    async fn test_save_inventory() {
        use crate::recording::proxy::save_inventory;

        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();

        let mut inventory = Inventory::new();
        inventory.entry_url = Some("https://example.com".to_string());
        inventory.device_type = Some(DeviceType::Mobile);

        // Add a test resource
        let resource =
            crate::types::Resource::new("GET".to_string(), "https://example.com".to_string());
        inventory.resources.push(resource);

        // Save the inventory
        save_inventory(&inventory, &inventory_dir).await.unwrap();

        // Check if the file was created
        let inventory_file = inventory_dir.join("inventory.json");
        assert!(inventory_file.exists());

        // Check if we can read it back
        let content = tokio::fs::read_to_string(&inventory_file).await.unwrap();
        let loaded_inventory: Inventory = serde_json::from_str(&content).unwrap();

        assert_eq!(
            loaded_inventory.entry_url,
            Some("https://example.com".to_string())
        );
        assert_eq!(loaded_inventory.device_type, Some(DeviceType::Mobile));
        assert_eq!(loaded_inventory.resources.len(), 1);
    }

    #[test]
    fn test_handle_proxy_request_creation() {
        // Test that we can create the basic request/response structure
        use crate::types::Resource;

        let resource = Resource::new("GET".to_string(), "https://example.com".to_string());

        assert_eq!(resource.method, "GET");
        assert_eq!(resource.url, "https://example.com");
        assert_eq!(resource.ttfb_ms, 0);
    }

    #[test]
    fn test_content_encoding_parsing() {
        use crate::types::ContentEncodingType;
        use std::str::FromStr;

        // Test gzip
        let gzip = ContentEncodingType::from_str("gzip").unwrap();
        assert!(matches!(gzip, ContentEncodingType::Gzip));

        // Test br (brotli)
        let br = ContentEncodingType::from_str("br").unwrap();
        assert!(matches!(br, ContentEncodingType::Br));

        // Test deflate
        let deflate = ContentEncodingType::from_str("deflate").unwrap();
        assert!(matches!(deflate, ContentEncodingType::Deflate));

        // Test identity
        let identity = ContentEncodingType::from_str("identity").unwrap();
        assert!(matches!(identity, ContentEncodingType::Identity));

        // Test case insensitivity
        let gzip_upper = ContentEncodingType::from_str("GZIP").unwrap();
        assert!(matches!(gzip_upper, ContentEncodingType::Gzip));

        // Test invalid encoding
        let invalid = ContentEncodingType::from_str("invalid-encoding");
        assert!(invalid.is_err());
    }
}
