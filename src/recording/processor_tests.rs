#[cfg(test)]
mod tests {
    use crate::recording::processor::RequestProcessor;
    use crate::traits::{
        FileSystem,
        mocks::{MockFileSystem, MockTimeProvider},
    };
    use crate::types::{ContentEncodingType, Resource};
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_process_response_body_html() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();

        let mock_fs = Arc::new(MockFileSystem::new());
        let mock_time = Arc::new(MockTimeProvider::new(1000));

        let processor = RequestProcessor::new(inventory_dir.clone(), mock_fs.clone(), mock_time);

        let mut resource = Resource::new(
            "GET".to_string(),
            "https://example.com/test.html".to_string(),
        );
        let html_content = b"<html><body><h1>Test</h1></body></html>";

        processor
            .process_response_body(
                &mut resource,
                html_content,
                Some("text/html; charset=utf-8"),
            )
            .await
            .unwrap();

        // Verify resource was updated
        assert_eq!(resource.content_type_mime, Some("text/html".to_string()));
        assert_eq!(resource.content_charset, Some("utf-8".to_string()));
        assert!(resource.content_file_path.is_some());
        assert!(resource.minify.is_some());
    }

    #[tokio::test]
    async fn test_process_text_resource() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();

        let mock_fs = Arc::new(MockFileSystem::new());
        let mock_time = Arc::new(MockTimeProvider::new(1000));

        let processor = RequestProcessor::new(inventory_dir.clone(), mock_fs.clone(), mock_time);

        let mut resource = Resource::new(
            "GET".to_string(),
            "https://example.com/script.js".to_string(),
        );
        resource.content_type_mime = Some("application/javascript".to_string());

        let js_content = b"function test() { return 42; }";

        processor
            .process_text_resource(&mut resource, js_content)
            .await
            .unwrap();

        // Verify file was "written"
        let expected_path = inventory_dir.join("contents/get/https/example.com/script.js");
        assert!(mock_fs.file_exists(&expected_path.to_string_lossy()));

        // Verify resource was updated
        assert!(resource.content_file_path.is_some());
    }

    #[tokio::test]
    async fn test_process_binary_resource() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();

        let mock_fs = Arc::new(MockFileSystem::new());
        let mock_time = Arc::new(MockTimeProvider::new(1000));

        let processor = RequestProcessor::new(inventory_dir.clone(), mock_fs.clone(), mock_time);

        let mut resource = Resource::new(
            "GET".to_string(),
            "https://example.com/image.png".to_string(),
        );
        let binary_content = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR"; // PNG header

        processor
            .process_binary_resource(&mut resource, binary_content)
            .await
            .unwrap();

        // Verify file was "written"
        let expected_path = inventory_dir.join("contents/get/https/example.com/image.png");
        assert!(mock_fs.file_exists(&expected_path.to_string_lossy()));

        // Verify resource was updated
        assert!(resource.content_file_path.is_some());
        assert!(resource.content_base64.is_some());
    }

    #[tokio::test]
    async fn test_decompress_gzip() {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::io::Write;

        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();

        let mock_fs = Arc::new(MockFileSystem::new());
        let mock_time = Arc::new(MockTimeProvider::new(1000));

        let processor = RequestProcessor::new(inventory_dir, mock_fs, mock_time);

        // Create gzipped content
        let original = b"Hello, World!";
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original).unwrap();
        let compressed = encoder.finish().unwrap();

        let result = processor
            .decompress_body(&compressed, &Some(ContentEncodingType::Gzip))
            .unwrap();
        assert_eq!(result, original);
    }

    #[test]
    fn test_convert_to_utf8() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();

        let mock_fs = Arc::new(MockFileSystem::new());
        let mock_time = Arc::new(MockTimeProvider::new(1000));

        let processor = RequestProcessor::new(inventory_dir, mock_fs, mock_time);

        let utf8_bytes = "Hello, 世界!".as_bytes();
        let (result, encoding_name) = processor.convert_to_utf8(utf8_bytes, &None);

        assert_eq!(result, "Hello, 世界!");
        assert_eq!(encoding_name, "UTF-8");
    }

    #[test]
    fn test_beautify_html() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();

        let mock_fs = Arc::new(MockFileSystem::new());
        let mock_time = Arc::new(MockTimeProvider::new(1000));

        let processor = RequestProcessor::new(inventory_dir, mock_fs, mock_time);

        let minified_html =
            "<html><head><title>Test</title></head><body><h1>Hello</h1></body></html>";
        let result = processor
            .beautify_content(minified_html, &Some("text/html".to_string()))
            .unwrap();

        // Should have more newlines
        assert!(result.lines().count() > minified_html.lines().count());
    }

    #[test]
    fn test_beautify_css() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();

        let mock_fs = Arc::new(MockFileSystem::new());
        let mock_time = Arc::new(MockTimeProvider::new(1000));

        let processor = RequestProcessor::new(inventory_dir, mock_fs, mock_time);

        let minified_css = "body{margin:0;padding:0;}h1{color:red;}";
        let result = processor
            .beautify_content(minified_css, &Some("text/css".to_string()))
            .unwrap();

        // Should have more structure
        assert!(result.contains('\n'));
        assert!(result.len() >= minified_css.len());
    }

    #[tokio::test]
    async fn test_original_charset_preservation() {
        let temp_dir = TempDir::new().unwrap();
        let inventory_dir = temp_dir.path().to_path_buf();

        let mock_fs = Arc::new(MockFileSystem::new());
        let mock_time = Arc::new(MockTimeProvider::new(1000));

        let processor = RequestProcessor::new(inventory_dir.clone(), mock_fs.clone(), mock_time);

        // Test with Shift_JIS charset
        let mut resource = Resource::new(
            "GET".to_string(),
            "https://example.com/index.html".to_string(),
        );
        resource.content_type_mime = Some("text/html".to_string());
        resource.content_charset = Some("Shift_JIS".to_string());

        // Create a simple HTML with Shift_JIS charset in meta
        let html = r#"<html><head><meta charset="Shift_JIS"><title>テスト</title></head><body>内容</body></html>"#;
        let body = html.as_bytes();

        processor
            .process_text_resource(&mut resource, body)
            .await
            .unwrap();

        // Verify content_charset is preserved (contains the original charset from Content-Type)
        assert_eq!(resource.content_charset, Some("Shift_JIS".to_string()));

        // Verify meta tag is PRESERVED (not modified to UTF-8)
        // Files are stored as UTF-8, but charset declarations remain as-is
        // During playback, content will be re-encoded to Shift_JIS based on resource.content_charset
        let file_path = inventory_dir.join(resource.content_file_path.as_ref().unwrap());
        let saved_content = mock_fs.read_to_string(&file_path).await.unwrap();
        assert!(
            saved_content.contains(r#"charset="Shift_JIS""#)
                || saved_content.contains(r#"charset='Shift_JIS'"#)
                || saved_content.contains(r#"charset=Shift_JIS"#),
            "Expected original charset declaration (Shift_JIS) to be preserved, but got: {}",
            saved_content
        );
    }
}
