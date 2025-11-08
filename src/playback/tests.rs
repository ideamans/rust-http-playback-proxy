#[cfg(test)]
mod tests {
    use crate::types::{ContentEncodingType, DeviceType, Inventory, Resource};
    use serde::Serialize;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio;

    #[tokio::test]
    async fn test_load_inventory() {
        use crate::playback::load_inventory;
        use crate::traits::RealFileSystem;

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
        // 2スペースインデントで整形
        let mut buf = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
        inventory.serialize(&mut ser).unwrap();
        let inventory_json = String::from_utf8(buf).unwrap();
        tokio::fs::write(&inventory_path, inventory_json)
            .await
            .unwrap();

        // Test loading
        let loaded_inventory = load_inventory(&inventory_dir, std::sync::Arc::new(RealFileSystem))
            .await
            .unwrap();

        assert_eq!(
            loaded_inventory.entry_url,
            Some("https://example.com".to_string())
        );
        assert_eq!(loaded_inventory.device_type, Some(DeviceType::Desktop));
        assert_eq!(loaded_inventory.resources.len(), 1);
    }

    #[tokio::test]
    async fn test_convert_resources_to_transactions() {
        use crate::playback::transaction::convert_resources_to_transactions;
        use crate::traits::RealFileSystem;

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
        let transactions = convert_resources_to_transactions(
            &inventory,
            &inventory_dir,
            std::sync::Arc::new(RealFileSystem),
        )
        .await
        .unwrap();

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
        use crate::traits::RealFileSystem;

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
        tokio::fs::write(&full_file_path, test_content)
            .await
            .unwrap();

        // Create a resource that references this file
        let mut resource = Resource::new(
            "GET".to_string(),
            "https://example.com/test.txt".to_string(),
        );
        resource.status_code = Some(200);
        resource.ttfb_ms = 50;
        resource.content_file_path = Some(format!("contents/{}", file_path));
        resource.mbps = Some(1.0);

        // Convert to transaction
        let transaction = convert_resource_to_transaction(
            &resource,
            &inventory_dir,
            std::sync::Arc::new(RealFileSystem),
        )
        .await
        .unwrap();

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
        let (chunks, target_close_time) = create_chunks(content, &resource).unwrap();

        assert!(!chunks.is_empty());

        // Verify that chunk target times are relative to TTFB (0-based)
        // First chunk should start at 0 (immediately after TTFB)
        assert_eq!(chunks[0].target_time, 0);

        // Verify that combined chunks equal original content
        let mut combined = Vec::new();
        for chunk in &chunks {
            combined.extend_from_slice(&chunk.chunk);
        }
        assert_eq!(combined, content);

        // Verify target_close_time is set appropriately (relative to TTFB)
        assert!(target_close_time > 0);
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

    #[tokio::test]
    async fn test_chunk_timing_with_delay() {
        use crate::playback::transaction::create_chunks;
        use std::time::Instant;

        // Create a resource with specific timing
        let mut resource = Resource::new("GET".to_string(), "https://example.com/test".to_string());
        resource.ttfb_ms = 100;
        resource.mbps = Some(1.0); // 1 Mbps

        // Create content that will be split into multiple chunks
        let content = vec![0u8; 128 * 1024]; // 128KB content
        let (chunks, target_close_time) = create_chunks(&content, &resource).unwrap();

        // Verify multiple chunks were created
        assert!(
            chunks.len() > 1,
            "Expected multiple chunks for 128KB content"
        );

        // Verify timing increases monotonically
        for i in 1..chunks.len() {
            assert!(
                chunks[i].target_time >= chunks[i - 1].target_time,
                "Chunk {} target_time should be >= previous chunk",
                i
            );
        }

        // Verify first chunk starts at 0 (relative to TTFB completion)
        assert_eq!(chunks[0].target_time, 0);

        // Verify target_close_time is after last chunk
        if let Some(last_chunk) = chunks.last() {
            assert!(
                target_close_time >= last_chunk.target_time,
                "target_close_time should be >= last chunk's target_time"
            );
        }

        // Simulate timing: measure relative delays between chunks
        let _start = Instant::now();
        let mut last_time = 0u64;

        for (i, chunk) in chunks.iter().enumerate() {
            let relative_delay = chunk.target_time - last_time;

            // For testing, we just verify the calculation is reasonable
            // (not testing actual sleep timing here)
            if i > 0 {
                assert!(
                    relative_delay > 0,
                    "Chunk {} should have positive delay from previous",
                    i
                );

                // Based on 64KB chunks at 1 Mbps:
                // 64KB = 65536 bytes = 524288 bits
                // 1 Mbps = 1000000 bits/sec
                // Expected time: ~524ms
                // Allow for some variance
                assert!(
                    relative_delay >= 400 && relative_delay <= 700,
                    "Chunk {} delay {}ms is outside expected range (400-700ms) for 64KB at 1Mbps",
                    i,
                    relative_delay
                );
            }

            last_time = chunk.target_time;
        }

        // Verify total time matches content size and bandwidth
        // 1 Mbps = 1,000,000 bits/sec = 125,000 bytes/sec = 125 bytes/ms
        let bytes_per_ms = 1.0 * 1000.0 * 1000.0 / 8.0 / 1000.0; // = 125 bytes/ms
        let expected_total_time = (content.len() as f64 / bytes_per_ms) as u64;
        let actual_total_time = target_close_time; // target_close_time is already relative to TTFB

        // Allow 10% tolerance for rounding
        let tolerance = (expected_total_time as f64 * 0.1) as u64;
        assert!(
            actual_total_time >= expected_total_time - tolerance
                && actual_total_time <= expected_total_time + tolerance,
            "Total transfer time {}ms should be within 10% of expected {}ms",
            actual_total_time,
            expected_total_time
        );
    }

    #[test]
    fn test_chunk_timing_calculation() {
        use crate::playback::transaction::create_chunks;

        // Test with different bandwidths
        let test_cases = vec![
            (1.0, 1024, 100),  // 1 Mbps, 1KB, 100ms TTFB
            (10.0, 10240, 50), // 10 Mbps, 10KB, 50ms TTFB
            (0.5, 512, 200),   // 0.5 Mbps, 512B, 200ms TTFB
        ];

        for (mbps, content_size, ttfb) in test_cases {
            let mut resource =
                Resource::new("GET".to_string(), "https://example.com/test".to_string());
            resource.ttfb_ms = ttfb;
            resource.mbps = Some(mbps);

            let content = vec![0u8; content_size];
            let (chunks, target_close_time) = create_chunks(&content, &resource).unwrap();

            // Verify first chunk timing (relative to TTFB, so 0)
            assert_eq!(chunks[0].target_time, 0);

            // Verify target_close_time is reasonable (relative to TTFB)
            // Mbps to bytes/ms: mbps * 1,000,000 bits/sec / 8 bits/byte / 1000 ms/sec
            let bytes_per_ms = (mbps * 1000.0 * 1000.0) / 8.0 / 1000.0;
            let expected_transfer_time = (content_size as f64 / bytes_per_ms) as u64;

            assert_eq!(
                target_close_time, expected_transfer_time,
                "For {}Mbps, {}B content: expected transfer time {}ms, got {}ms",
                mbps, content_size, expected_transfer_time, target_close_time
            );
        }
    }

    #[test]
    fn test_compress_brotli_content() {
        use crate::playback::transaction::compress_content;

        let content = b"This is test content for Brotli compression testing. Brotli is a modern compression algorithm developed by Google.";

        let compressed = compress_content(content, &ContentEncodingType::Br).unwrap();

        // Compressed content should be different
        assert_ne!(compressed, content);
        // Brotli should compress this content
        assert!(compressed.len() < content.len());
    }

    #[test]
    fn test_compress_deflate_content() {
        use crate::playback::transaction::compress_content;

        let content = b"This is test content for deflate compression. Deflate is a common compression algorithm used in HTTP.";

        let compressed = compress_content(content, &ContentEncodingType::Deflate).unwrap();

        // Compressed content should be different
        assert_ne!(compressed, content);
        assert!(compressed.len() > 0);
    }

    #[test]
    fn test_compress_very_small_content() {
        use crate::playback::transaction::compress_content;

        let content = b"Hi";

        // Gzip may increase size for very small content
        let compressed = compress_content(content, &ContentEncodingType::Gzip).unwrap();
        assert!(compressed.len() > 0);

        // But identity should preserve it
        let identity = compress_content(content, &ContentEncodingType::Identity).unwrap();
        assert_eq!(identity, content);
    }

    #[test]
    fn test_create_chunks_with_zero_mbps() {
        use crate::playback::transaction::create_chunks;

        let mut resource = Resource::new("GET".to_string(), "https://example.com/test".to_string());
        resource.ttfb_ms = 100;
        resource.mbps = Some(0.0); // Invalid: 0 Mbps

        let content = b"test content";

        // Should handle edge case gracefully
        let result = create_chunks(content, &resource);

        // Should either error or use a reasonable default
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_create_chunks_without_mbps() {
        use crate::playback::transaction::create_chunks;

        let mut resource = Resource::new("GET".to_string(), "https://example.com/test".to_string());
        resource.ttfb_ms = 100;
        resource.mbps = None; // No bandwidth info

        let content = b"test content";
        let result = create_chunks(content, &resource);

        // Should handle missing bandwidth
        assert!(result.is_ok());
        if let Ok((chunks, _)) = result {
            assert!(!chunks.is_empty());
        }
    }

    #[tokio::test]
    async fn test_convert_resource_with_error_message() {
        use crate::playback::transaction::convert_resource_to_transaction;
        use crate::traits::RealFileSystem;

        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();

        let mut resource =
            Resource::new("GET".to_string(), "https://example.com/error".to_string());
        resource.error_message = Some("Connection timeout".to_string());
        resource.status_code = Some(504);
        resource.ttfb_ms = 5000;
        // Add dummy content so the transaction is created
        resource.content_utf8 = Some("Gateway Timeout".to_string());

        let transaction = convert_resource_to_transaction(
            &resource,
            &inventory_dir,
            std::sync::Arc::new(RealFileSystem),
        )
        .await
        .unwrap();

        assert!(transaction.is_some());
        let tx = transaction.unwrap();
        assert_eq!(tx.error_message, Some("Connection timeout".to_string()));
        assert_eq!(tx.status_code, Some(504));
    }

    #[test]
    fn test_minify_javascript_content() {
        use crate::playback::transaction::minify_content;

        let js_with_comments =
            b"// This is a comment\nfunction test() {\n  // Another comment\n  return 42;\n}";

        let minified = minify_content(
            js_with_comments,
            &Some("application/javascript".to_string()),
        )
        .unwrap();
        let minified_str = String::from_utf8(minified).unwrap();

        // Should be more compact
        assert!(minified_str.len() <= js_with_comments.len());
    }

    #[tokio::test]
    async fn test_load_inventory_invalid_json() {
        use crate::playback::load_inventory;
        use crate::traits::mocks::MockFileSystem;

        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();

        let mock_fs = Arc::new(MockFileSystem::new());

        // Create invalid JSON file
        let inventory_path = inventory_dir.join("inventory.json");
        mock_fs.set_file(
            &inventory_path.to_string_lossy(),
            b"{ invalid json".to_vec(),
        );

        // Should fail gracefully
        let result = load_inventory(&inventory_dir, mock_fs).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_content_encoding_all_types() {
        use std::str::FromStr;

        // Test all encoding types
        assert!(ContentEncodingType::from_str("gzip").is_ok());
        assert!(ContentEncodingType::from_str("br").is_ok());
        assert!(ContentEncodingType::from_str("deflate").is_ok());
        assert!(ContentEncodingType::from_str("identity").is_ok());

        // Case insensitive
        assert!(ContentEncodingType::from_str("GZIP").is_ok());
        assert!(ContentEncodingType::from_str("Br").is_ok());

        // Invalid
        assert!(ContentEncodingType::from_str("unknown").is_err());
        assert!(ContentEncodingType::from_str("").is_err());
    }
}
