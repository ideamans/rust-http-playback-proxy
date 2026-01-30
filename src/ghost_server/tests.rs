//! Ghost Server Tests

use crate::ghost_server::GhostServer;
use crate::types::{BodyChunk, Transaction};

fn create_test_transaction(method: &str, url: &str, status: u16, content: &[u8]) -> Transaction {
    Transaction {
        method: method.to_string(),
        url: url.to_string(),
        ttfb: 0, // No delay for tests
        status_code: Some(status),
        error_message: None,
        raw_headers: Some(std::collections::HashMap::from([(
            "content-type".to_string(),
            crate::types::HeaderValue::Single("text/plain".to_string()),
        )])),
        chunks: vec![BodyChunk {
            chunk: content.to_vec(),
            target_time: 0,
        }],
        target_close_time: 0,
    }
}

#[tokio::test]
async fn test_ghost_server_serves_transaction() {
    let transactions = vec![create_test_transaction(
        "GET",
        "http://example.com/test",
        200,
        b"Hello, World!",
    )];

    let server = GhostServer::start(0, transactions, false)
        .await
        .expect("Failed to start server");

    let port = server.port();

    // Make request
    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://127.0.0.1:{}/test", port))
        .header("Host", "example.com")
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 200);
    let body = response.text().await.expect("Failed to read body");
    assert_eq!(body, "Hello, World!");

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_ghost_server_returns_404_for_unknown() {
    let transactions = vec![create_test_transaction(
        "GET",
        "http://example.com/known",
        200,
        b"Known resource",
    )];

    let server = GhostServer::start(0, transactions, false)
        .await
        .expect("Failed to start server");

    let port = server.port();

    // Make request to unknown path
    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://127.0.0.1:{}/unknown", port))
        .header("Host", "example.com")
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 404);

    // Verify X-Ghost-Miss header is set on miss responses
    let ghost_miss = response.headers().get("x-ghost-miss");
    assert!(
        ghost_miss.is_some(),
        "Expected X-Ghost-Miss header on 404 miss"
    );
    assert_eq!(ghost_miss.unwrap().to_str().unwrap(), "true");

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_ghost_server_miss_header_not_on_recorded_404() {
    // A recorded 404 response should NOT have X-Ghost-Miss header
    let transactions = vec![create_test_transaction(
        "GET",
        "http://example.com/not-found",
        404,
        b"Page not found",
    )];

    let server = GhostServer::start(0, transactions, false)
        .await
        .expect("Failed to start server");

    let port = server.port();

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://127.0.0.1:{}/not-found", port))
        .header("Host", "example.com")
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 404);

    // Recorded 404 should NOT have X-Ghost-Miss header
    let ghost_miss = response.headers().get("x-ghost-miss");
    assert!(
        ghost_miss.is_none(),
        "Recorded 404 should not have X-Ghost-Miss header"
    );

    let body = response.text().await.expect("Failed to read body");
    assert_eq!(body, "Page not found");

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_ghost_server_host_matching() {
    let transactions = vec![
        create_test_transaction("GET", "http://example.com/test", 200, b"Example"),
        create_test_transaction("GET", "http://other.com/test", 200, b"Other"),
    ];

    let server = GhostServer::start(0, transactions, false)
        .await
        .expect("Failed to start server");

    let port = server.port();
    let client = reqwest::Client::new();

    // Request with example.com host
    let response = client
        .get(format!("http://127.0.0.1:{}/test", port))
        .header("Host", "example.com")
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 200);
    let body = response.text().await.expect("Failed to read body");
    assert_eq!(body, "Example");

    // Request with other.com host
    let response = client
        .get(format!("http://127.0.0.1:{}/test", port))
        .header("Host", "other.com")
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 200);
    let body = response.text().await.expect("Failed to read body");
    assert_eq!(body, "Other");

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_ghost_server_query_matching() {
    let transactions = vec![
        create_test_transaction("GET", "http://example.com/search?q=rust", 200, b"Rust"),
        create_test_transaction("GET", "http://example.com/search?q=go", 200, b"Go"),
    ];

    let server = GhostServer::start(0, transactions, false)
        .await
        .expect("Failed to start server");

    let port = server.port();
    let client = reqwest::Client::new();

    // Request with q=rust
    let response = client
        .get(format!("http://127.0.0.1:{}/search?q=rust", port))
        .header("Host", "example.com")
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 200);
    let body = response.text().await.expect("Failed to read body");
    assert_eq!(body, "Rust");

    // Request with q=go
    let response = client
        .get(format!("http://127.0.0.1:{}/search?q=go", port))
        .header("Host", "example.com")
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 200);
    let body = response.text().await.expect("Failed to read body");
    assert_eq!(body, "Go");

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_ghost_server_method_matching() {
    let transactions = vec![
        create_test_transaction("GET", "http://example.com/api", 200, b"GET response"),
        create_test_transaction("POST", "http://example.com/api", 201, b"POST response"),
    ];

    let server = GhostServer::start(0, transactions, false)
        .await
        .expect("Failed to start server");

    let port = server.port();
    let client = reqwest::Client::new();

    // GET request
    let response = client
        .get(format!("http://127.0.0.1:{}/api", port))
        .header("Host", "example.com")
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 200);
    let body = response.text().await.expect("Failed to read body");
    assert_eq!(body, "GET response");

    // POST request
    let response = client
        .post(format!("http://127.0.0.1:{}/api", port))
        .header("Host", "example.com")
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 201);
    let body = response.text().await.expect("Failed to read body");
    assert_eq!(body, "POST response");

    server.stop().await.expect("Failed to stop server");
}

/// Helper to create a transaction with explicit timing parameters
fn create_timed_transaction(
    method: &str,
    url: &str,
    status: u16,
    content: &[u8],
    ttfb_ms: u64,
    chunk_target_time: u64,
    target_close_time: u64,
) -> Transaction {
    Transaction {
        method: method.to_string(),
        url: url.to_string(),
        ttfb: ttfb_ms,
        status_code: Some(status),
        error_message: None,
        raw_headers: Some(std::collections::HashMap::from([(
            "content-type".to_string(),
            crate::types::HeaderValue::Single("text/plain".to_string()),
        )])),
        chunks: vec![BodyChunk {
            chunk: content.to_vec(),
            target_time: chunk_target_time,
        }],
        target_close_time,
    }
}

#[tokio::test]
async fn test_full_throttle_serves_correct_content() {
    // Transaction with large delays that would make the test slow without full_throttle
    let transactions = vec![create_timed_transaction(
        "GET",
        "http://example.com/slow",
        200,
        b"Full throttle content",
        5000,  // 5s TTFB
        3000,  // 3s chunk delay
        10000, // 10s close time
    )];

    let server = GhostServer::start(0, transactions, true)
        .await
        .expect("Failed to start server");

    let port = server.port();
    let client = reqwest::Client::new();

    let start = std::time::Instant::now();

    let response = client
        .get(format!("http://127.0.0.1:{}/slow", port))
        .header("Host", "example.com")
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 200);
    let body = response.text().await.expect("Failed to read body");
    assert_eq!(body, "Full throttle content");

    let elapsed = start.elapsed();

    // With full_throttle, should complete well under 1 second
    // (without it, TTFB alone would take 5 seconds)
    assert!(
        elapsed.as_millis() < 1000,
        "Full throttle should skip timing delays, but took {}ms",
        elapsed.as_millis()
    );

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_full_throttle_skips_chunk_timing() {
    // Create multiple chunks with large delays
    let chunks = vec![
        BodyChunk {
            chunk: b"chunk1".to_vec(),
            target_time: 0,
        },
        BodyChunk {
            chunk: b"chunk2".to_vec(),
            target_time: 3000, // 3s delay
        },
        BodyChunk {
            chunk: b"chunk3".to_vec(),
            target_time: 6000, // 6s delay
        },
    ];

    let transactions = vec![Transaction {
        method: "GET".to_string(),
        url: "http://example.com/chunked".to_string(),
        ttfb: 2000, // 2s TTFB
        status_code: Some(200),
        error_message: None,
        raw_headers: Some(std::collections::HashMap::from([(
            "content-type".to_string(),
            crate::types::HeaderValue::Single("text/plain".to_string()),
        )])),
        chunks,
        target_close_time: 9000, // 9s close time
    }];

    let server = GhostServer::start(0, transactions, true)
        .await
        .expect("Failed to start server");

    let port = server.port();
    let client = reqwest::Client::new();

    let start = std::time::Instant::now();

    let response = client
        .get(format!("http://127.0.0.1:{}/chunked", port))
        .header("Host", "example.com")
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 200);
    let body = response.text().await.expect("Failed to read body");
    assert_eq!(body, "chunk1chunk2chunk3");

    let elapsed = start.elapsed();

    // Without full_throttle this would take ~9 seconds (2s TTFB + 6s chunks + close wait)
    assert!(
        elapsed.as_millis() < 1000,
        "Full throttle should skip all timing, but took {}ms",
        elapsed.as_millis()
    );

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_without_full_throttle_respects_ttfb() {
    // Verify that timing IS applied when full_throttle is false
    let transactions = vec![create_timed_transaction(
        "GET",
        "http://example.com/timed",
        200,
        b"Timed response",
        500, // 500ms TTFB
        0,
        0,
    )];

    let server = GhostServer::start(0, transactions, false)
        .await
        .expect("Failed to start server");

    let port = server.port();
    let client = reqwest::Client::new();

    let start = std::time::Instant::now();

    let response = client
        .get(format!("http://127.0.0.1:{}/timed", port))
        .header("Host", "example.com")
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 200);
    let body = response.text().await.expect("Failed to read body");
    assert_eq!(body, "Timed response");

    let elapsed = start.elapsed();

    // With throttling, should take at least 400ms (500ms TTFB with some tolerance)
    assert!(
        elapsed.as_millis() >= 400,
        "Without full_throttle, TTFB should be respected, but completed in {}ms",
        elapsed.as_millis()
    );

    server.stop().await.expect("Failed to stop server");
}
