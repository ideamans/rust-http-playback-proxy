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

    let server = GhostServer::start(0, transactions)
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

    let server = GhostServer::start(0, transactions)
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

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_ghost_server_host_matching() {
    let transactions = vec![
        create_test_transaction("GET", "http://example.com/test", 200, b"Example"),
        create_test_transaction("GET", "http://other.com/test", 200, b"Other"),
    ];

    let server = GhostServer::start(0, transactions)
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

    let server = GhostServer::start(0, transactions)
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

    let server = GhostServer::start(0, transactions)
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
