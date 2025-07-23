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
}