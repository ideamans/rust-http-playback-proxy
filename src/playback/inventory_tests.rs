#[cfg(test)]
mod tests {
    use crate::playback::load_inventory;
    use crate::traits::mocks::MockFileSystem;
    use crate::recording::proxy::save_inventory_with_fs;
    use crate::types::{Inventory, Resource, DeviceType};
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_save_and_load_inventory() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();
        
        let mock_fs = Arc::new(MockFileSystem::new());
        
        // Create test inventory
        let mut inventory = Inventory::new();
        inventory.entry_url = Some("https://example.com".to_string());
        inventory.device_type = Some(DeviceType::Desktop);
        
        let mut resource = Resource::new("GET".to_string(), "https://example.com/api/test".to_string());
        resource.status_code = Some(200);
        resource.ttfb_ms = 150;
        resource.mbps = Some(1.5);
        resource.content_utf8 = Some("{ \"test\": \"data\" }".to_string());
        
        inventory.resources.push(resource);
        
        // Save inventory
        save_inventory_with_fs(&inventory, &inventory_dir, mock_fs.clone())
            .await
            .unwrap();
        
        // Verify file was created
        let inventory_path = inventory_dir.join("inventory.json").to_string_lossy().to_string();
        assert!(mock_fs.file_exists(&inventory_path));
        
        // Load inventory back
        let loaded_inventory = load_inventory(&inventory_dir, mock_fs).await.unwrap();
        
        // Verify loaded data matches
        assert_eq!(loaded_inventory.entry_url, Some("https://example.com".to_string()));
        assert_eq!(loaded_inventory.device_type, Some(DeviceType::Desktop));
        assert_eq!(loaded_inventory.resources.len(), 1);
        
        let loaded_resource = &loaded_inventory.resources[0];
        assert_eq!(loaded_resource.method, "GET");
        assert_eq!(loaded_resource.url, "https://example.com/api/test");
        assert_eq!(loaded_resource.status_code, Some(200));
        assert_eq!(loaded_resource.ttfb_ms, 150);
        assert_eq!(loaded_resource.content_utf8, Some("{ \"test\": \"data\" }".to_string()));
    }

    #[tokio::test]
    async fn test_load_inventory_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();
        
        let mock_fs = Arc::new(MockFileSystem::new());
        
        // Try to load non-existent inventory
        let result = load_inventory(&inventory_dir, mock_fs).await;
        
        // Should fail with error
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_save_inventory_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf().join("non-existent");
        
        let mock_fs = Arc::new(MockFileSystem::new());
        
        let inventory = Inventory::new();
        
        // Save inventory to non-existent directory
        let result = save_inventory_with_fs(&inventory, &inventory_dir, mock_fs.clone()).await;
        
        // Should succeed
        assert!(result.is_ok());
        
        // Directory should have been created
        let inventory_path = inventory_dir.join("inventory.json").to_string_lossy().to_string();
        assert!(mock_fs.file_exists(&inventory_path));
    }

    #[tokio::test] 
    async fn test_inventory_serialization_format() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();
        
        let mock_fs = Arc::new(MockFileSystem::new());
        
        let mut inventory = Inventory::new();
        inventory.entry_url = Some("https://test.com".to_string());
        inventory.device_type = Some(DeviceType::Mobile);
        
        let mut resource = Resource::new("POST".to_string(), "https://test.com/api".to_string());
        resource.status_code = Some(201);
        resource.ttfb_ms = 75;
        
        inventory.resources.push(resource);
        
        // Save inventory
        save_inventory_with_fs(&inventory, &inventory_dir, mock_fs.clone())
            .await
            .unwrap();
        
        // Get the saved JSON
        let inventory_path = inventory_dir.join("inventory.json").to_string_lossy().to_string();
        let saved_json = mock_fs.get_file(&inventory_path).unwrap();
        let json_str = String::from_utf8(saved_json).unwrap();
        
        // Verify JSON structure
        assert!(json_str.contains("entryUrl"));
        assert!(json_str.contains("deviceType"));
        assert!(json_str.contains("resources"));
        assert!(json_str.contains("https://test.com"));
        assert!(json_str.contains("mobile"));
        assert!(json_str.contains("POST"));
        assert!(json_str.contains("201"));
        assert!(json_str.contains("75"));
        
        // Verify 2-space indentation
        assert!(json_str.contains("{\n  \"entryUrl\""));
    }

    #[tokio::test]
    async fn test_empty_inventory() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();
        
        let mock_fs = Arc::new(MockFileSystem::new());
        
        let empty_inventory = Inventory::new();
        
        // Save empty inventory
        save_inventory_with_fs(&empty_inventory, &inventory_dir, mock_fs.clone())
            .await
            .unwrap();
        
        // Load it back
        let loaded = load_inventory(&inventory_dir, mock_fs).await.unwrap();
        
        // Should match empty state
        assert!(loaded.entry_url.is_none());
        assert!(loaded.device_type.is_none());
        assert!(loaded.resources.is_empty());
    }

    #[tokio::test]
    async fn test_inventory_with_complex_resource() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();
        
        let mock_fs = Arc::new(MockFileSystem::new());
        
        let mut inventory = Inventory::new();
        
        // Create resource with all fields populated
        let mut resource = Resource::new("PUT".to_string(), "https://api.example.com/data?id=123".to_string());
        resource.status_code = Some(204);
        resource.ttfb_ms = 300;
        resource.mbps = Some(0.5);
        resource.error_message = Some("Rate limited".to_string());
        
        use std::collections::HashMap;
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), crate::types::HeaderValue::Single("application/json".to_string()));
        headers.insert("x-rate-limit".to_string(), crate::types::HeaderValue::Single("100".to_string()));
        resource.raw_headers = Some(headers);
        
        resource.content_encoding = Some(crate::types::ContentEncodingType::Gzip);
        resource.content_type_mime = Some("application/json".to_string());
        resource.content_type_charset = Some("UTF-8".to_string());
        resource.content_file_path = Some("put/https/api.example.com/data~id=123.json".to_string());
        resource.minify = Some(true);
        
        inventory.resources.push(resource);
        
        // Save and load
        save_inventory_with_fs(&inventory, &inventory_dir, mock_fs.clone())
            .await
            .unwrap();
        
        let loaded = load_inventory(&inventory_dir, mock_fs).await.unwrap();
        
        // Verify complex resource was preserved
        assert_eq!(loaded.resources.len(), 1);
        let loaded_resource = &loaded.resources[0];
        
        assert_eq!(loaded_resource.method, "PUT");
        assert_eq!(loaded_resource.url, "https://api.example.com/data?id=123");
        assert_eq!(loaded_resource.status_code, Some(204));
        assert_eq!(loaded_resource.ttfb_ms, 300);
        assert_eq!(loaded_resource.mbps, Some(0.5));
        assert_eq!(loaded_resource.error_message, Some("Rate limited".to_string()));
        assert!(loaded_resource.raw_headers.is_some());
        assert_eq!(loaded_resource.content_encoding, Some(crate::types::ContentEncodingType::Gzip));
        assert_eq!(loaded_resource.minify, Some(true));
    }

    #[tokio::test]
    async fn test_json_indentation_format() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();
        
        let mock_fs = Arc::new(MockFileSystem::new());
        
        let mut inventory = Inventory::new();
        inventory.entry_url = Some("https://example.com".to_string());
        inventory.device_type = Some(DeviceType::Desktop);
        
        let mut resource = Resource::new("GET".to_string(), "https://example.com/api".to_string());
        resource.status_code = Some(200);
        resource.ttfb_ms = 100;
        
        inventory.resources.push(resource);
        
        // Save inventory
        save_inventory_with_fs(&inventory, &inventory_dir, mock_fs.clone())
            .await
            .unwrap();
        
        // Get the saved JSON
        let inventory_path = inventory_dir.join("inventory.json").to_string_lossy().to_string();
        let saved_json = mock_fs.get_file(&inventory_path).unwrap();
        let json_str = String::from_utf8(saved_json).unwrap();
        
        println!("Generated JSON format:");
        println!("{}", json_str);
        
        // 2スペースインデントの確認
        assert!(json_str.contains("{\n  \"entryUrl\""));
        assert!(json_str.contains("  \"deviceType\""));
        assert!(json_str.contains("  \"resources\""));
        assert!(json_str.contains("    \"method\""));  // リソース内のフィールドは4スペース(2レベル)
        assert!(json_str.contains("    \"url\""));
        
        // 4スペースではないことを確認
        assert!(!json_str.contains("{\n    \"entryUrl\""));
    }
}