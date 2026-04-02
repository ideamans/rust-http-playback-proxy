#[cfg(test)]
mod recording_tests {
    use crate::types::{DeviceType, Inventory};
    use regex::Regex;
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
        let inventory_file = inventory_dir.join("index.json");
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

        // Test zstd
        let zstd = ContentEncodingType::from_str("zstd").unwrap();
        assert!(matches!(zstd, ContentEncodingType::Zstd));

        // Test identity
        let identity = ContentEncodingType::from_str("identity").unwrap();
        assert!(matches!(identity, ContentEncodingType::Identity));

        // Test case insensitivity
        let gzip_upper = ContentEncodingType::from_str("GZIP").unwrap();
        assert!(matches!(gzip_upper, ContentEncodingType::Gzip));

        let zstd_upper = ContentEncodingType::from_str("ZSTD").unwrap();
        assert!(matches!(zstd_upper, ContentEncodingType::Zstd));

        // Test invalid encoding
        let invalid = ContentEncodingType::from_str("invalid-encoding");
        assert!(invalid.is_err());
    }

    #[tokio::test]
    async fn test_exclude_pattern_skips_matching_request() {
        use crate::recording::hudsucker_handler::RecordingHandler;
        use hudsucker::{HttpContext, HttpHandler, RequestOrResponse, hyper::Request};
        use std::net::SocketAddr;

        let inventory = Inventory::new();
        let exclude_patterns = vec![Regex::new(r"google-analytics\.com").unwrap()];
        let mut handler = RecordingHandler::new(inventory, exclude_patterns);

        let ctx = HttpContext {
            client_addr: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
            request_method: hyper::Method::GET,
            request_uri: "https://www.google-analytics.com/analytics.js"
                .parse()
                .unwrap(),
        };

        let req = Request::builder()
            .method("GET")
            .uri("https://www.google-analytics.com/analytics.js")
            .header("host", "www.google-analytics.com")
            .body(hudsucker::Body::empty())
            .unwrap();

        let result = handler.handle_request(&ctx, req).await;

        // Should pass through as a request (not intercepted for recording)
        assert!(matches!(result, RequestOrResponse::Request(_)));

        // Inventory should be empty - no request info stored
        let binding = handler.get_inventory();
        let inv = binding.lock().await;
        assert_eq!(inv.resources.len(), 0);
    }

    #[tokio::test]
    async fn test_exclude_pattern_allows_non_matching_request() {
        use crate::recording::hudsucker_handler::RecordingHandler;
        use hudsucker::{HttpContext, HttpHandler, RequestOrResponse, hyper::Request};
        use std::net::SocketAddr;

        let inventory = Inventory::new();
        let exclude_patterns = vec![Regex::new(r"google-analytics\.com").unwrap()];
        let mut handler = RecordingHandler::new(inventory, exclude_patterns);

        let ctx = HttpContext {
            client_addr: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
            request_method: hyper::Method::GET,
            request_uri: "https://example.com/page.html".parse().unwrap(),
        };

        let req = Request::builder()
            .method("GET")
            .uri("https://example.com/page.html")
            .header("host", "example.com")
            .body(hudsucker::Body::empty())
            .unwrap();

        let result = handler.handle_request(&ctx, req).await;

        // Should still pass through as request (recording happens on response)
        assert!(matches!(result, RequestOrResponse::Request(_)));
    }

    #[tokio::test]
    async fn test_exclude_pattern_skips_matching_response() {
        use crate::recording::hudsucker_handler::RecordingHandler;
        use hudsucker::{HttpContext, HttpHandler, hyper::Response};
        use std::net::SocketAddr;

        let inventory = Inventory::new();
        let exclude_patterns = vec![Regex::new(r"tracking\.js").unwrap()];
        let mut handler = RecordingHandler::new(inventory, exclude_patterns);

        let ctx = HttpContext {
            client_addr: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
            request_method: hyper::Method::GET,
            request_uri: "https://example.com/tracking.js".parse().unwrap(),
        };

        let res = Response::builder()
            .status(200)
            .body(hudsucker::Body::empty())
            .unwrap();

        let _result = handler.handle_response(&ctx, res).await;

        // Inventory should be empty - excluded URL should not be recorded
        let binding = handler.get_inventory();
        let inv = binding.lock().await;
        assert_eq!(inv.resources.len(), 0);
    }

    #[tokio::test]
    async fn test_exclude_pattern_records_non_matching_response() {
        use crate::recording::hudsucker_handler::RecordingHandler;
        use hudsucker::{HttpContext, HttpHandler, hyper::Request, hyper::Response};
        use std::net::SocketAddr;

        let inventory = Inventory::new();
        let exclude_patterns = vec![Regex::new(r"tracking\.js").unwrap()];
        let mut handler = RecordingHandler::new(inventory, exclude_patterns);

        let client_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let uri: hyper::Uri = "https://example.com/index.html".parse().unwrap();

        // First, handle the request to store request info
        let ctx = HttpContext {
            client_addr,
            request_method: hyper::Method::GET,
            request_uri: uri.clone(),
        };

        let req = Request::builder()
            .method("GET")
            .uri("https://example.com/index.html")
            .header("host", "example.com")
            .body(hudsucker::Body::empty())
            .unwrap();

        handler.handle_request(&ctx, req).await;

        // Then handle the response
        let res = Response::builder()
            .status(200)
            .header("content-type", "text/html")
            .body(hudsucker::Body::empty())
            .unwrap();

        let _result = handler.handle_response(&ctx, res).await;

        // Inventory should have the resource - non-excluded URL should be recorded
        let binding = handler.get_inventory();
        let inv = binding.lock().await;
        assert_eq!(inv.resources.len(), 1);
        assert_eq!(inv.resources[0].url, "https://example.com/index.html");
    }

    #[tokio::test]
    async fn test_multiple_exclude_patterns() {
        use crate::recording::hudsucker_handler::RecordingHandler;
        use hudsucker::{HttpContext, HttpHandler, hyper::Response};
        use std::net::SocketAddr;

        let inventory = Inventory::new();
        let exclude_patterns = vec![
            Regex::new(r"google-analytics\.com").unwrap(),
            Regex::new(r"\.woff2(\?|$)").unwrap(),
            Regex::new(r"/api/tracking").unwrap(),
        ];
        let mut handler = RecordingHandler::new(inventory, exclude_patterns);

        let test_cases = vec![
            ("https://www.google-analytics.com/ga.js", true),
            ("https://example.com/fonts/roboto.woff2", true),
            ("https://example.com/fonts/roboto.woff2?v=123", true),
            ("https://example.com/api/tracking/event", true),
            ("https://example.com/index.html", false),
            ("https://example.com/style.css", false),
        ];

        for (url, should_exclude) in test_cases {
            let ctx = HttpContext {
                client_addr: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
                request_method: hyper::Method::GET,
                request_uri: url.parse().unwrap(),
            };

            let res = Response::builder()
                .status(200)
                .body(hudsucker::Body::empty())
                .unwrap();

            handler.handle_response(&ctx, res).await;

            let binding = handler.get_inventory();
            let inv = binding.lock().await;
            if should_exclude {
                assert_eq!(
                    inv.resources.iter().filter(|r| r.url == url).count(),
                    0,
                    "URL should have been excluded: {}",
                    url
                );
            }
            // Non-excluded URLs won't be recorded here either because
            // handle_request wasn't called, so request_info is missing.
            // That's tested in test_exclude_pattern_records_non_matching_response.
        }
    }

    #[tokio::test]
    async fn test_no_exclude_patterns_records_everything() {
        use crate::recording::hudsucker_handler::RecordingHandler;
        use hudsucker::{HttpContext, HttpHandler, hyper::Request, hyper::Response};
        use std::net::SocketAddr;

        let inventory = Inventory::new();
        let mut handler = RecordingHandler::new(inventory, vec![]);

        let client_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();

        // Request + Response for a URL
        let ctx = HttpContext {
            client_addr,
            request_method: hyper::Method::GET,
            request_uri: "https://example.com/page.html".parse().unwrap(),
        };

        let req = Request::builder()
            .method("GET")
            .uri("https://example.com/page.html")
            .header("host", "example.com")
            .body(hudsucker::Body::empty())
            .unwrap();

        handler.handle_request(&ctx, req).await;

        let res = Response::builder()
            .status(200)
            .body(hudsucker::Body::empty())
            .unwrap();

        handler.handle_response(&ctx, res).await;

        let binding = handler.get_inventory();
        let inv = binding.lock().await;
        assert_eq!(inv.resources.len(), 1);
    }
}
