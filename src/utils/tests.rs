#[cfg(test)]
mod tests {
    use crate::utils::{find_available_port, get_port_or_default, generate_file_path_from_url, 
               is_text_resource, extract_charset_from_content_type};

    #[test]
    #[allow(unused_comparisons)]
    fn test_find_available_port() {
        let port = find_available_port(8080).unwrap();
        assert!(port >= 8080);
        // u16の最大値は65535なので、この比較は常にtrueになるが意図的に残す
        assert!(port <= 65535);
    }

    #[test]
    fn test_get_port_or_default() {
        let port = get_port_or_default(Some(9090)).unwrap();
        assert_eq!(port, 9090);

        let default_port = get_port_or_default(None).unwrap();
        assert!(default_port >= 8080);
    }

    #[test]
    fn test_generate_file_path_from_url_simple() {
        let result = generate_file_path_from_url("https://example.com/", "GET").unwrap();
        assert_eq!(result, "get/https/example.com/index.html");
    }

    #[test]
    fn test_generate_file_path_from_url_with_path() {
        let result = generate_file_path_from_url("https://example.com/path/to/resource.js", "GET").unwrap();
        assert_eq!(result, "get/https/example.com/path/to/resource.js");
    }

    #[test]
    fn test_generate_file_path_from_url_with_short_query() {
        let result = generate_file_path_from_url("https://example.com/path/resource?param=value", "GET").unwrap();
        assert_eq!(result, "get/https/example.com/path/resource~param%3Dvalue");
    }

    #[test]
    fn test_generate_file_path_from_url_with_long_query() {
        let long_query = "a".repeat(40);
        let result = generate_file_path_from_url(&format!("https://example.com/resource?{}", long_query), "GET").unwrap();
        
        assert!(result.starts_with("get/https/example.com/resource~"));
        assert!(result.contains(".~"));
    }

    #[test]
    fn test_generate_file_path_from_url_with_extension() {
        let result = generate_file_path_from_url("https://example.com/script.js?v=1", "GET").unwrap();
        assert_eq!(result, "get/https/example.com/script~v%3D1.js");
    }

    #[test]
    fn test_is_text_resource() {
        assert!(is_text_resource("text/html; charset=utf-8"));
        assert!(is_text_resource("text/css"));
        assert!(is_text_resource("application/javascript"));
        assert!(is_text_resource("text/javascript"));
        assert!(!is_text_resource("image/png"));
        assert!(!is_text_resource("application/octet-stream"));
    }

    #[test]
    fn test_extract_charset_from_content_type() {
        assert_eq!(
            extract_charset_from_content_type("text/html; charset=utf-8"),
            Some("utf-8".to_string())
        );
        assert_eq!(
            extract_charset_from_content_type("text/html; charset=\"utf-8\""),
            Some("utf-8".to_string())
        );
        assert_eq!(
            extract_charset_from_content_type("text/html; charset=shift_jis; boundary=something"),
            Some("shift_jis".to_string())
        );
        assert_eq!(
            extract_charset_from_content_type("text/html"),
            None
        );
        assert_eq!(
            extract_charset_from_content_type("application/json"),
            None
        );
    }

    #[test]
    fn test_generate_file_path_query_hash() {
        // Test exact 32-character boundary
        let query_32_chars = "a".repeat(32);
        let url = format!("https://example.com/test?{}", query_32_chars);
        let result = generate_file_path_from_url(&url, "GET").unwrap();
        // Exactly 32 chars should not trigger hashing
        assert_eq!(result, format!("get/https/example.com/test~{}", query_32_chars));

        // Test 33 characters - should trigger hashing
        let query_33_chars = "a".repeat(33);
        let url = format!("https://example.com/test?{}", query_33_chars);
        let result = generate_file_path_from_url(&url, "GET").unwrap();
        // Should contain hash marker
        assert!(result.contains(".~"));
        assert!(result.starts_with("get/https/example.com/test~"));

        // Test very long query
        let query_long = "param=".to_string() + &"x".repeat(100);
        let url = format!("https://example.com/api/endpoint?{}", query_long);
        let result = generate_file_path_from_url(&url, "GET").unwrap();
        assert!(result.contains(".~"));
    }

    #[test]
    fn test_generate_file_path_multiple_query_params() {
        let result = generate_file_path_from_url("https://example.com/search?q=rust&page=1&sort=date", "GET").unwrap();
        assert!(result.contains("~"));
        assert!(result.contains("q%3Drust"));
    }

    #[test]
    fn test_generate_file_path_special_chars() {
        let result = generate_file_path_from_url("https://example.com/path with spaces.html", "GET").unwrap();
        assert!(result.contains("path"));

        let result = generate_file_path_from_url("https://example.com/日本語.html", "GET").unwrap();
        assert!(result.contains("get/https/example.com"));
    }

    #[test]
    fn test_generate_file_path_methods() {
        let url = "https://api.example.com/data";

        assert_eq!(
            generate_file_path_from_url(url, "GET").unwrap(),
            "get/https/api.example.com/data"
        );

        assert_eq!(
            generate_file_path_from_url(url, "POST").unwrap(),
            "post/https/api.example.com/data"
        );

        assert_eq!(
            generate_file_path_from_url(url, "DELETE").unwrap(),
            "delete/https/api.example.com/data"
        );
    }

    #[test]
    fn test_is_text_resource_extended() {
        // Test supported text content types (based on actual implementation)
        assert!(is_text_resource("text/html"));
        assert!(is_text_resource("text/html; charset=utf-8"));
        assert!(is_text_resource("text/css"));
        assert!(is_text_resource("text/css; charset=UTF-8"));
        assert!(is_text_resource("application/javascript"));
        assert!(is_text_resource("application/javascript; charset=utf-8"));
        assert!(is_text_resource("text/javascript"));

        // Non-text types (not explicitly supported)
        assert!(!is_text_resource("text/plain"));
        assert!(!is_text_resource("application/json"));
        assert!(!is_text_resource("application/xml"));
        assert!(!is_text_resource("image/jpeg"));
        assert!(!is_text_resource("image/webp"));
        assert!(!is_text_resource("video/mp4"));
        assert!(!is_text_resource("audio/mpeg"));
        assert!(!is_text_resource("application/pdf"));
        assert!(!is_text_resource("application/zip"));
    }

    #[test]
    fn test_extract_charset_edge_cases() {
        // Uppercase charset parameter name
        assert_eq!(
            extract_charset_from_content_type("text/html; CHARSET=UTF-8"),
            Some("UTF-8".to_string())
        );

        // Mixed case parameter name
        assert_eq!(
            extract_charset_from_content_type("text/html; Charset=ISO-8859-1"),
            Some("ISO-8859-1".to_string())
        );

        // With quotes
        assert_eq!(
            extract_charset_from_content_type("text/html; charset=\"UTF-8\""),
            Some("UTF-8".to_string())
        );

        // Multiple parameters
        assert_eq!(
            extract_charset_from_content_type("multipart/form-data; boundary=----WebKitFormBoundary; charset=utf-8"),
            Some("utf-8".to_string())
        );

        // Charset value preservation (not converted to lowercase)
        assert_eq!(
            extract_charset_from_content_type("text/html; charset=Shift_JIS"),
            Some("Shift_JIS".to_string())
        );
    }
}