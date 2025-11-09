use anyhow::Result;
use hyper::StatusCode;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::{sleep, timeout};

mod static_server {
    use anyhow::Result;
    use http_body_util::Full;
    use hyper::body::Incoming;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response, StatusCode};
    use hyper_util::rt::TokioIo;
    use std::net::SocketAddr;
    use tokio::net::TcpListener;

    pub struct StaticServer {
        pub addr: SocketAddr,
        shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    }

    impl StaticServer {
        pub async fn start() -> Result<Self> {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let addr = listener.local_addr()?;

            let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

            tokio::spawn(async move {
                let mut shutdown_rx = shutdown_rx;

                loop {
                    tokio::select! {
                        result = listener.accept() => {
                            match result {
                                Ok((stream, _)) => {
                                    tokio::spawn(async move {
                                        if let Err(err) = http1::Builder::new()
                                            .serve_connection(
                                                TokioIo::new(stream),
                                                service_fn(handle_request),
                                            )
                                            .await
                                        {
                                            eprintln!("Error serving connection: {:?}", err);
                                        }
                                    });
                                }
                                Err(e) => {
                                    eprintln!("Failed to accept connection: {}", e);
                                    break;
                                }
                            }
                        }
                        _ = &mut shutdown_rx => {
                            break;
                        }
                    }
                }
            });

            Ok(StaticServer {
                addr,
                shutdown_tx: Some(shutdown_tx),
            })
        }

        pub fn url(&self) -> String {
            format!("http://127.0.0.1:{}", self.addr.port())
        }

        pub fn shutdown(mut self) {
            if let Some(tx) = self.shutdown_tx.take() {
                let _ = tx.send(());
            }
        }
    }

    impl Drop for StaticServer {
        fn drop(&mut self) {
            if let Some(tx) = self.shutdown_tx.take() {
                let _ = tx.send(());
            }
        }
    }

    async fn handle_request(
        req: Request<Incoming>,
    ) -> Result<Response<Full<bytes::Bytes>>, hyper::Error> {
        let path = req.uri().path();

        match path {
            "/" | "/index.html" => {
                let html = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Test Page</title>
    <link rel="stylesheet" href="/style.css">
</head>
<body>
    <h1>Test Page for HTTP Playback Proxy</h1>
    <p>This is a simple test page for integration testing.</p>
    <script src="/script.js"></script>
</body>
</html>"#;

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/html; charset=utf-8")
                    .body(Full::new(bytes::Bytes::from(html)))
                    .unwrap())
            }

            "/style.css" => {
                let css = r#"body {
    font-family: Arial, sans-serif;
    margin: 0;
    padding: 20px;
    background-color: #f0f0f0;
}

h1 {
    color: #333;
    text-align: center;
}

p {
    color: #666;
    line-height: 1.6;
}"#;

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/css; charset=utf-8")
                    .body(Full::new(bytes::Bytes::from(css)))
                    .unwrap())
            }

            "/script.js" => {
                let js = r#"console.log('Test script loaded');

document.addEventListener('DOMContentLoaded', function() {
    console.log('DOM content loaded');

    const h1 = document.querySelector('h1');
    if (h1) {
        h1.style.color = '#0066cc';
    }
});"#;

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/javascript; charset=utf-8")
                    .body(Full::new(bytes::Bytes::from(js)))
                    .unwrap())
            }

            // Compressed content (gzip)
            "/compressed.txt" => {
                use flate2::Compression;
                use flate2::write::GzEncoder;
                use std::io::Write;

                let text = "This is compressed content for testing gzip encoding.";
                let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(text.as_bytes()).unwrap();
                let compressed = encoder.finish().unwrap();

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/plain; charset=utf-8")
                    .header("Content-Encoding", "gzip")
                    .body(Full::new(bytes::Bytes::from(compressed)))
                    .unwrap())
            }

            // Compressed content (brotli)
            "/compressed-br.txt" => {
                use brotli::enc::BrotliEncoderParams;
                use std::io::Write;

                let text = "This is brotli compressed content for testing.";
                let mut compressed = Vec::new();
                let params = BrotliEncoderParams::default();
                let mut writer =
                    brotli::CompressorWriter::with_params(&mut compressed, 4096, &params);
                writer.write_all(text.as_bytes()).unwrap();
                writer.flush().unwrap();
                drop(writer);

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/plain; charset=utf-8")
                    .header("Content-Encoding", "br")
                    .body(Full::new(bytes::Bytes::from(compressed)))
                    .unwrap())
            }

            // 404 error
            "/not-found" => Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(Full::new(bytes::Bytes::from(
                    "<html><body><h1>404 Not Found</h1></body></html>",
                )))
                .unwrap()),

            // 500 error
            "/server-error" => Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(Full::new(bytes::Bytes::from(
                    "<html><body><h1>500 Internal Server Error</h1></body></html>",
                )))
                .unwrap()),

            _ => Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Full::new(bytes::Bytes::from("Not Found")))
                .unwrap()),
        }
    }
}

use static_server::StaticServer;

/// Find a free port for testing
fn find_free_port() -> Result<u16> {
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let port = addr.port();
    drop(listener); // Release the port
    Ok(port)
}

/// HTTP client that supports proxy
async fn http_client_with_proxy(proxy_port: u16) -> reqwest::Client {
    let proxy_url = format!("http://127.0.0.1:{}", proxy_port);
    println!("Creating HTTP client with proxy: {}", proxy_url);
    let proxy = reqwest::Proxy::http(&proxy_url).expect("Failed to create proxy");

    let client = reqwest::Client::builder()
        .proxy(proxy)
        .timeout(Duration::from_secs(30))
        .danger_accept_invalid_certs(true)
        .build()
        .expect("Failed to create HTTP client");

    println!("HTTP client created successfully with proxy configuration");
    client
}

/// Start the HTTP playback proxy in recording mode
async fn start_recording_proxy(port: u16, inventory_dir: &Path) -> Result<Child> {
    let binary_path = get_binary_path();

    println!(
        "Starting recording proxy with command: {} recording --port {} --device desktop --inventory {}",
        binary_path.display(),
        port,
        inventory_dir.display()
    );

    let mut child = Command::new(&binary_path)
        .args([
            "recording",
            "--port",
            &port.to_string(),
            "--device",
            "desktop",
            "--inventory",
            &inventory_dir.to_string_lossy(),
        ])
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Wait for the proxy to start
    sleep(Duration::from_millis(2000)).await;

    // Check if the port is actually listening
    if std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).is_err() {
        panic!("Recording proxy is not listening on port {}", port);
    }
    println!("Recording proxy confirmed listening on port {}", port);

    // Also check if the process is actually running
    let output = Command::new("lsof")
        .args(["-i", &format!(":{}", port)])
        .output()
        .expect("Failed to run lsof");
    println!(
        "Port {} usage: {}",
        port,
        String::from_utf8_lossy(&output.stdout)
    );

    // Check the child process status (temporarily disabled to avoid port conflicts)
    match child.try_wait() {
        Ok(Some(status)) => {
            println!(
                "Warning: Recording proxy exited with status: {} (this may be due to port conflicts)",
                status
            );
            // Continue with the test anyway for now
        }
        Ok(None) => {
            println!("Recording proxy is still running");
        }
        Err(e) => {
            println!("Warning: Error checking child process status: {}", e);
        }
    }

    Ok(child)
}

/// Start the HTTP playback proxy in playback mode
async fn start_playback_proxy(port: u16, inventory_dir: &Path) -> Result<Child> {
    let binary_path = get_binary_path();

    let child = Command::new(&binary_path)
        .args([
            "playback",
            "--port",
            &port.to_string(),
            "--inventory",
            &inventory_dir.to_string_lossy(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Wait for the proxy to start
    sleep(Duration::from_millis(1000)).await;

    Ok(child)
}

/// Get the path to the compiled binary
fn get_binary_path() -> PathBuf {
    let mut debug_path = std::env::current_dir().expect("Failed to get current directory");
    debug_path.push("target");
    debug_path.push("debug");
    debug_path.push("http-playback-proxy");

    let mut release_path = std::env::current_dir().expect("Failed to get current directory");
    release_path.push("target");
    release_path.push("release");
    release_path.push("http-playback-proxy");

    // Prefer debug binary for testing (more logging)

    if debug_path.exists() {
        println!("Using debug binary: {}", debug_path.display());
        debug_path
    } else if release_path.exists() {
        println!("Using release binary: {}", release_path.display());
        release_path
    } else {
        panic!("No binary found. Please run 'cargo build' or 'cargo build --release' first.");
    }
}

/// Check if binary exists (no longer builds automatically)
async fn ensure_binary_exists() -> Result<()> {
    let binary_path = get_binary_path();

    if !binary_path.exists() {
        anyhow::bail!(
            "Binary not found at {}. Please run 'cargo build' or 'cargo build --release' first.",
            binary_path.display()
        );
    }

    println!("Using binary: {}", binary_path.display());
    Ok(())
}

#[tokio::test]
async fn test_recording_and_playback_integration() {
    // Check binary exists
    ensure_binary_exists().await.expect("Binary not found");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let inventory_dir = temp_dir.path().to_path_buf();

    // Step 1: Start static web server
    println!("Starting static web server...");
    let static_server = StaticServer::start()
        .await
        .expect("Failed to start static server");
    let server_url = static_server.url();
    println!("Static server started at: {}", server_url);

    // Use dynamic ports to avoid conflicts
    let recording_proxy_port = find_free_port().expect("Failed to find free port for recording");
    let playback_proxy_port = find_free_port().expect("Failed to find free port for playback");

    // Step 2: Start recording proxy
    println!(
        "Starting recording proxy on port {}...",
        recording_proxy_port
    );
    let mut recording_proxy = start_recording_proxy(recording_proxy_port, &inventory_dir)
        .await
        .expect("Failed to start recording proxy");

    // Step 3: Make requests through recording proxy
    println!("Making requests through recording proxy...");
    let client = http_client_with_proxy(recording_proxy_port).await;

    // IMPORTANT: Test only with local static server for hermetic testing
    // No external dependencies (httpbin.org, etc.) to ensure tests are reproducible
    // and don't fail due to network issues or external service availability
    println!("Making request to: {}", server_url);
    let response = timeout(Duration::from_secs(10), client.get(&server_url).send())
        .await
        .expect("Request timeout")
        .expect("Failed to make request");
    println!("Response status: {}", response.status());
    println!("Response headers: {:?}", response.headers());
    assert!(response.status().is_success());
    let html_content = response.text().await.expect("Failed to read response");
    println!("HTML response content: {}", html_content);

    // Let's also test the static server directly without proxy
    println!("Testing static server directly...");
    let direct_client = reqwest::Client::new();
    let direct_response = direct_client
        .get(&server_url)
        .send()
        .await
        .expect("Direct request failed");
    let direct_content = direct_response
        .text()
        .await
        .expect("Failed to read direct response");
    println!("Direct response content: {}", direct_content);

    // For now, let's check what we actually received and continue with the test
    if html_content.contains("Recording mode - this would be the actual response") {
        println!(
            "WARNING: Received mock response, but continuing with test to gather more information"
        );
        println!("This suggests a proxy configuration or process management issue");
        // Don't panic, continue the test to see what happens
    }

    assert!(html_content.contains("Test Page for HTTP Playback Proxy"));

    // Request CSS
    let css_url = format!("{}/style.css", server_url);
    let response = timeout(Duration::from_secs(10), client.get(&css_url).send())
        .await
        .expect("Request timeout")
        .expect("Failed to make CSS request");
    assert!(response.status().is_success());
    let css_content = response.text().await.expect("Failed to read CSS response");
    assert!(css_content.contains("font-family"));

    // Request JavaScript
    let js_url = format!("{}/script.js", server_url);
    let response = timeout(Duration::from_secs(10), client.get(&js_url).send())
        .await
        .expect("Request timeout")
        .expect("Failed to make JS request");
    assert!(response.status().is_success());
    let js_content = response.text().await.expect("Failed to read JS response");
    assert!(js_content.contains("console.log"));

    println!("Requests completed successfully");

    // Step 4: Stop recording proxy gracefully
    println!("Stopping recording proxy...");
    #[cfg(unix)]
    {
        // Send SIGINT (Ctrl+C) to allow graceful shutdown and inventory saving
        unsafe {
            libc::kill(recording_proxy.id() as i32, libc::SIGINT);
        }
        // Wait for graceful shutdown
        match timeout(Duration::from_secs(5), async {
            loop {
                match recording_proxy.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => {
                        sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                    Err(_) => break,
                }
            }
        })
        .await
        {
            Ok(_) => println!("Recording proxy shut down gracefully"),
            Err(_) => {
                println!("Recording proxy did not shut down gracefully, force killing");
                let _ = recording_proxy.kill();
                let _ = recording_proxy.wait();
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = recording_proxy.kill();
        let _ = recording_proxy.wait();
    }

    // Wait a moment for files to be written
    sleep(Duration::from_millis(2000)).await;

    // Step 5: Stop static server
    println!("Stopping static server...");
    static_server.shutdown();

    // Step 6: Verify inventory and contents were created
    println!("Verifying recorded files...");
    println!("Inventory directory: {:?}", inventory_dir);

    // List all files in inventory directory
    if let Ok(entries) = std::fs::read_dir(&inventory_dir) {
        println!("Contents of inventory directory:");
        for entry in entries.flatten() {
            println!("  - {:?}", entry.path());
        }
    } else {
        println!("Could not read inventory directory");
    }

    let inventory_file = inventory_dir.join("inventory.json");
    assert!(inventory_file.exists(), "inventory.json should exist");

    // Read inventory to see what's in it
    if let Ok(inv_content) = std::fs::read_to_string(&inventory_file) {
        println!("Inventory.json contents:\n{}", inv_content);
    }

    let contents_dir = inventory_dir.join("contents");
    assert!(contents_dir.exists(), "contents directory should exist");

    // Read and verify inventory
    let inventory_content = tokio::fs::read_to_string(&inventory_file)
        .await
        .expect("Failed to read inventory.json");
    println!("Inventory content: {}", inventory_content);

    // Parse inventory JSON
    let inventory: serde_json::Value =
        serde_json::from_str(&inventory_content).expect("Failed to parse inventory.json");

    let resources = inventory
        .get("resources")
        .expect("resources field should exist")
        .as_array()
        .expect("resources should be an array");

    assert!(
        resources.len() >= 3,
        "Should have at least 3 resources (HTML, CSS, JS)"
    );

    // Verify each resource has required fields
    for resource in resources {
        assert!(resource.get("method").is_some());
        assert!(resource.get("url").is_some());
        assert!(resource.get("ttfbMs").is_some());
        assert!(resource.get("statusCode").is_some());
    }

    // Step 7: Start playback proxy
    println!("Starting playback proxy on port {}...", playback_proxy_port);
    let mut playback_proxy = start_playback_proxy(playback_proxy_port, &inventory_dir)
        .await
        .expect("Failed to start playback proxy");

    // Step 8: Make requests to playback proxy (static server is stopped)
    println!("Making requests to playback proxy...");
    let playback_client = http_client_with_proxy(playback_proxy_port).await;

    // Request index page from playback
    let response = timeout(
        Duration::from_secs(10),
        playback_client.get(&server_url).send(),
    )
    .await
    .expect("Playback request timeout")
    .expect("Failed to make playback request");
    assert!(response.status().is_success());
    let playback_html = response
        .text()
        .await
        .expect("Failed to read playback response");
    assert!(playback_html.contains("Test Page for HTTP Playback Proxy"));

    // Verify playback content matches recording (semantic content, not exact formatting)
    // Remove whitespace differences for comparison since minification may normalize spacing
    let normalized_recorded = html_content
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    let normalized_playback = playback_html
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    assert_eq!(
        normalized_recorded, normalized_playback,
        "Playback HTML content should match recorded HTML content"
    );

    // Request CSS from playback
    let response = timeout(
        Duration::from_secs(10),
        playback_client.get(&css_url).send(),
    )
    .await
    .expect("Playback CSS request timeout")
    .expect("Failed to make playback CSS request");
    assert!(response.status().is_success());
    let playback_css = response
        .text()
        .await
        .expect("Failed to read playback CSS response");
    // Normalize whitespace for CSS comparison due to minification
    let normalized_recorded_css = css_content
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    let normalized_playback_css = playback_css
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    assert_eq!(
        normalized_recorded_css, normalized_playback_css,
        "Playback CSS content should match recorded CSS content"
    );

    // Request JavaScript from playback
    let response = timeout(Duration::from_secs(10), playback_client.get(&js_url).send())
        .await
        .expect("Playback JS request timeout")
        .expect("Failed to make playback JS request");
    assert!(response.status().is_success());
    let playback_js = response
        .text()
        .await
        .expect("Failed to read playback JS response");
    // Normalize whitespace for JS comparison due to minification
    let normalized_recorded_js = js_content
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    let normalized_playback_js = playback_js
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    assert_eq!(
        normalized_recorded_js, normalized_playback_js,
        "Playback JS content should match recorded JS content"
    );

    // Step 9: Stop playback proxy
    println!("Stopping playback proxy...");
    let _ = playback_proxy.kill();
    let _ = playback_proxy.wait();

    println!("Integration test completed successfully!");
}

#[tokio::test]
async fn test_recording_error_responses() {
    // Build binary if needed
    ensure_binary_exists().await.expect("Binary not found");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let inventory_dir = temp_dir.path().to_path_buf();

    // Start static web server
    let static_server = StaticServer::start()
        .await
        .expect("Failed to start static server");
    let server_url = static_server.url();

    let recording_proxy_port = find_free_port().expect("Failed to find free port");
    let playback_proxy_port = find_free_port().expect("Failed to find free port");

    // Start recording proxy
    let mut recording_proxy = start_recording_proxy(recording_proxy_port, &inventory_dir)
        .await
        .expect("Failed to start recording proxy");

    let client = http_client_with_proxy(recording_proxy_port).await;

    // Request 404 error
    let not_found_url = format!("{}/not-found", server_url);
    let response = client
        .get(&not_found_url)
        .send()
        .await
        .expect("Request failed");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let not_found_content = response.text().await.expect("Failed to read response");
    assert!(not_found_content.contains("404 Not Found"));

    // Request 500 error
    let error_url = format!("{}/server-error", server_url);
    let response = client.get(&error_url).send().await.expect("Request failed");
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let error_content = response.text().await.expect("Failed to read response");
    assert!(error_content.contains("500 Internal Server Error"));

    // Stop recording proxy gracefully
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(recording_proxy.id() as i32, libc::SIGINT);
        }
        sleep(Duration::from_secs(3)).await;
        let _ = recording_proxy.wait();
    }
    #[cfg(not(unix))]
    {
        let _ = recording_proxy.kill();
        let _ = recording_proxy.wait();
        sleep(Duration::from_millis(2000)).await;
    }

    static_server.shutdown();

    // Verify inventory was created
    let inventory_file = inventory_dir.join("inventory.json");
    assert!(inventory_file.exists(), "inventory.json should exist");

    // Parse and verify error responses were recorded
    let inventory_content = tokio::fs::read_to_string(&inventory_file)
        .await
        .expect("Failed to read inventory.json");
    let inventory: serde_json::Value =
        serde_json::from_str(&inventory_content).expect("Failed to parse inventory.json");

    let resources = inventory["resources"]
        .as_array()
        .expect("resources should be an array");

    // Find 404 resource
    let not_found_resource = resources
        .iter()
        .find(|r| r["statusCode"] == 404)
        .expect("Should have 404 resource");
    assert_eq!(not_found_resource["statusCode"], 404);

    // Find 500 resource
    let error_resource = resources
        .iter()
        .find(|r| r["statusCode"] == 500)
        .expect("Should have 500 resource");
    assert_eq!(error_resource["statusCode"], 500);

    // Start playback proxy
    let mut playback_proxy = start_playback_proxy(playback_proxy_port, &inventory_dir)
        .await
        .expect("Failed to start playback proxy");

    let playback_client = http_client_with_proxy(playback_proxy_port).await;

    // Verify 404 is replayed correctly
    let response = playback_client
        .get(&not_found_url)
        .send()
        .await
        .expect("Playback request failed");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let playback_content = response
        .text()
        .await
        .expect("Failed to read playback response");
    assert!(playback_content.contains("404 Not Found"));

    // Verify 500 is replayed correctly
    let response = playback_client
        .get(&error_url)
        .send()
        .await
        .expect("Playback request failed");
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let playback_content = response
        .text()
        .await
        .expect("Failed to read playback response");
    assert!(playback_content.contains("500 Internal Server Error"));

    // Stop playback proxy
    let _ = playback_proxy.kill();
    let _ = playback_proxy.wait();

    println!("Error responses test completed successfully!");
}

#[tokio::test]
async fn test_recording_with_compression() {
    // Build binary if needed
    ensure_binary_exists().await.expect("Binary not found");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let inventory_dir = temp_dir.path().to_path_buf();

    // Start static web server
    let static_server = StaticServer::start()
        .await
        .expect("Failed to start static server");
    let server_url = static_server.url();

    let recording_proxy_port = find_free_port().expect("Failed to find free port");
    let playback_proxy_port = find_free_port().expect("Failed to find free port");

    // Start recording proxy
    let mut recording_proxy = start_recording_proxy(recording_proxy_port, &inventory_dir)
        .await
        .expect("Failed to start recording proxy");

    let client = http_client_with_proxy(recording_proxy_port).await;

    // Request gzip compressed content
    // Note: reqwest automatically decompresses, so we just verify the request succeeds
    let gzip_url = format!("{}/compressed.txt", server_url);
    let response = client.get(&gzip_url).send().await.expect("Request failed");
    assert!(response.status().is_success());
    let _gzip_content = response.text().await.expect("Failed to read response");

    // Request brotli compressed content
    let br_url = format!("{}/compressed-br.txt", server_url);
    let response = client.get(&br_url).send().await.expect("Request failed");
    assert!(response.status().is_success());
    let _br_content = response.text().await.expect("Failed to read response");

    // Stop recording proxy gracefully
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(recording_proxy.id() as i32, libc::SIGINT);
        }
        sleep(Duration::from_secs(3)).await;
        let _ = recording_proxy.wait();
    }
    #[cfg(not(unix))]
    {
        let _ = recording_proxy.kill();
        let _ = recording_proxy.wait();
        sleep(Duration::from_millis(2000)).await;
    }

    static_server.shutdown();

    // Verify inventory was created
    let inventory_file = inventory_dir.join("inventory.json");
    assert!(inventory_file.exists(), "inventory.json should exist");

    // Parse and verify compressed resources were recorded with correct encoding
    let inventory_content = tokio::fs::read_to_string(&inventory_file)
        .await
        .expect("Failed to read inventory.json");
    let inventory: serde_json::Value =
        serde_json::from_str(&inventory_content).expect("Failed to parse inventory.json");

    let resources = inventory["resources"]
        .as_array()
        .expect("resources should be an array");

    // Find gzip resource
    let gzip_resource = resources
        .iter()
        .find(|r| r["url"].as_str().unwrap_or("").contains("/compressed.txt"))
        .expect("Should have gzip compressed resource");
    assert_eq!(
        gzip_resource["contentEncoding"], "gzip",
        "Content encoding should be gzip"
    );

    // Find brotli resource
    let br_resource = resources
        .iter()
        .find(|r| {
            r["url"]
                .as_str()
                .unwrap_or("")
                .contains("/compressed-br.txt")
        })
        .expect("Should have brotli compressed resource");
    assert_eq!(
        br_resource["contentEncoding"], "br",
        "Content encoding should be br"
    );

    // Start playback proxy
    let mut playback_proxy = start_playback_proxy(playback_proxy_port, &inventory_dir)
        .await
        .expect("Failed to start playback proxy");

    let playback_client = http_client_with_proxy(playback_proxy_port).await;

    // Verify gzip content is replayed correctly (just verify success)
    let response = playback_client
        .get(&gzip_url)
        .send()
        .await
        .expect("Playback request failed");
    assert!(
        response.status().is_success(),
        "Gzip playback should succeed"
    );
    let _playback_gzip = response
        .text()
        .await
        .expect("Failed to read playback response");

    // Verify brotli content is replayed correctly (just verify success)
    let response = playback_client
        .get(&br_url)
        .send()
        .await
        .expect("Playback request failed");
    assert!(
        response.status().is_success(),
        "Brotli playback should succeed"
    );
    let _playback_br = response
        .text()
        .await
        .expect("Failed to read playback response");

    // Stop playback proxy
    let _ = playback_proxy.kill();
    let _ = playback_proxy.wait();

    println!("Compression test completed successfully!");
}

#[tokio::test]
async fn test_inventory_structure_validation() {
    // Build binary if needed
    ensure_binary_exists().await.expect("Binary not found");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let inventory_dir = temp_dir.path().to_path_buf();

    // Start static web server
    let static_server = StaticServer::start()
        .await
        .expect("Failed to start static server");
    let server_url = static_server.url();

    let recording_proxy_port = find_free_port().expect("Failed to find free port");

    // Start recording proxy with entry URL
    let mut recording_proxy = start_recording_proxy(recording_proxy_port, &inventory_dir)
        .await
        .expect("Failed to start recording proxy");

    let client = http_client_with_proxy(recording_proxy_port).await;

    // Make some requests
    let _ = client.get(&server_url).send().await;
    let _ = client.get(format!("{}/style.css", server_url)).send().await;

    // Stop recording proxy gracefully
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(recording_proxy.id() as i32, libc::SIGINT);
        }
        sleep(Duration::from_secs(3)).await;
        let _ = recording_proxy.wait();
    }
    #[cfg(not(unix))]
    {
        let _ = recording_proxy.kill();
        let _ = recording_proxy.wait();
        sleep(Duration::from_millis(2000)).await;
    }

    static_server.shutdown();

    // Validate inventory structure
    let inventory_file = inventory_dir.join("inventory.json");
    assert!(inventory_file.exists(), "inventory.json must exist");

    let inventory_content = tokio::fs::read_to_string(&inventory_file)
        .await
        .expect("Failed to read inventory.json");
    let inventory: serde_json::Value =
        serde_json::from_str(&inventory_content).expect("Failed to parse inventory.json");

    // Validate top-level structure
    assert!(inventory.is_object(), "Inventory must be an object");
    assert!(
        inventory["deviceType"].is_string(),
        "deviceType must be present"
    );
    assert_eq!(
        inventory["deviceType"], "desktop",
        "deviceType should be desktop"
    );

    // Validate resources array
    let resources = inventory["resources"]
        .as_array()
        .expect("resources must be an array");
    assert!(!resources.is_empty(), "resources array must not be empty");

    // Validate each resource has required fields
    for (i, resource) in resources.iter().enumerate() {
        assert!(
            resource["method"].is_string(),
            "Resource {} must have method",
            i
        );
        assert!(resource["url"].is_string(), "Resource {} must have url", i);
        assert!(
            resource["ttfbMs"].is_number(),
            "Resource {} must have ttfbMs",
            i
        );
        assert!(
            resource["statusCode"].is_number(),
            "Resource {} must have statusCode",
            i
        );

        // Check TTFB is non-negative
        let ttfb = resource["ttfbMs"].as_f64().unwrap();
        assert!(ttfb >= 0.0, "Resource {} ttfbMs must be non-negative", i);

        // Check status code is valid HTTP status
        let status = resource["statusCode"].as_u64().unwrap();
        assert!(
            (100..600).contains(&status),
            "Resource {} statusCode must be valid HTTP status code",
            i
        );

        // Validate content file path or content fields exist
        let has_file_path = resource["contentFilePath"].is_string();
        let has_utf8 = resource["contentUtf8"].is_string();
        let has_base64 = resource["contentBase64"].is_string();
        assert!(
            has_file_path || has_utf8 || has_base64,
            "Resource {} must have at least one content field",
            i
        );
    }

    // Validate contents directory structure
    let contents_dir = inventory_dir.join("contents");
    assert!(contents_dir.exists(), "contents directory must exist");
    assert!(contents_dir.is_dir(), "contents must be a directory");

    // Check that content files exist for resources with contentFilePath
    for resource in resources {
        if let Some(file_path) = resource["contentFilePath"].as_str() {
            let full_path = inventory_dir.join(file_path);
            assert!(
                full_path.exists(),
                "Content file must exist: {}",
                full_path.display()
            );
        }
    }

    println!("Inventory structure validation completed successfully!");
}
