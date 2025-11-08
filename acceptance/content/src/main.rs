use anyhow::Result;
use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Incoming;
use std::convert::Infallible;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::time::sleep;
use tracing::{error, info};

// Minified test content
const MINIFIED_HTML: &str = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Test</title><link rel="stylesheet" href="/style.css"></head><body><div class="container"><h1>Test Page</h1><p>This is a test page with minified content.</p><ul><li>Item 1</li><li>Item 2</li><li>Item 3</li></ul></div><script src="/script.js"></script></body></html>"#;

const MINIFIED_CSS: &str = r#"body{margin:0;padding:0;font-family:Arial,sans-serif}.container{max-width:1200px;margin:0 auto;padding:20px}h1{color:#333;font-size:2em;margin-bottom:1em}p{line-height:1.6;color:#666}ul{list-style:none;padding:0}li{padding:10px;margin:5px 0;background:#f0f0f0;border-radius:5px}"#;

const MINIFIED_JS: &str = r#"(function(){"use strict";function init(){console.log("Initialized");document.addEventListener("DOMContentLoaded",function(){var items=document.querySelectorAll("li");items.forEach(function(item,index){item.addEventListener("click",function(){alert("Clicked item "+(index+1))})})});}init();})();"#;

// Inventory types (matching the main project)
#[derive(Debug, Serialize, Deserialize)]
struct Inventory {
    resources: Vec<Resource>,
    #[serde(rename = "entryUrl")]
    entry_url: Option<String>,
    #[serde(rename = "deviceType")]
    device_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Resource {
    method: String,
    url: String,
    #[serde(rename = "statusCode")]
    status_code: Option<u16>,
    minify: Option<bool>,
}

// Mock HTTP server handler
async fn handle_request(
    req: Request<Incoming>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
    let path = req.uri().path();
    info!("Mock server received request for: {}", path);

    let (content_type, body) = match path {
        "/" => ("text/html; charset=utf-8", MINIFIED_HTML),
        "/style.css" => ("text/css; charset=utf-8", MINIFIED_CSS),
        "/script.js" => ("application/javascript; charset=utf-8", MINIFIED_JS),
        _ => {
            let response = Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Full::new(Bytes::from("Not Found")).boxed())
                .unwrap();
            return Ok(response);
        }
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        .body(Full::new(Bytes::from(body)).boxed())
        .unwrap();

    Ok(response)
}

// Start mock HTTP server
async fn start_mock_server(port: u16) -> Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;

    info!("Mock HTTP server listening on http://{}", addr);

    loop {
        let (stream, _) = listener.accept().await?;

        tokio::spawn(async move {
            let service = service_fn(handle_request);

            if let Err(err) = Builder::new(TokioExecutor::new())
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                error!("Error serving connection: {:?}", err);
            }
        });
    }
}

// Start recording proxy
fn start_recording_proxy(
    entry_url: &str,
    proxy_port: u16,
    inventory_dir: &PathBuf,
) -> Result<Child> {
    let binary_path = std::env::current_dir()?
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/release/http-playback-proxy");

    if !binary_path.exists() {
        anyhow::bail!(
            "Binary not found at {:?}. Please run 'cargo build --release' first.",
            binary_path
        );
    }

    let child = Command::new(binary_path)
        .arg("recording")
        .arg(entry_url)
        .arg("--port")
        .arg(proxy_port.to_string())
        .arg("--inventory")
        .arg(inventory_dir.to_str().unwrap())
        .spawn()?;

    Ok(child)
}

// Make HTTP request through proxy
async fn make_request(proxy_port: u16, url: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::http(format!(
            "http://127.0.0.1:{}",
            proxy_port
        ))?)
        .build()?;

    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Request failed with status: {}", response.status());
    }

    let _body = response.text().await?;

    Ok(())
}

// Count lines in a string
fn count_lines(content: &str) -> usize {
    content.lines().count()
}

// Verify that content was beautified
fn verify_beautified_content(inventory_dir: &PathBuf) -> Result<()> {
    info!("\n--- Verifying Beautified Content ---");

    let contents_dir = inventory_dir.join("contents");
    if !contents_dir.exists() {
        anyhow::bail!("Contents directory not found: {:?}", contents_dir);
    }

    // Check HTML
    let html_path = contents_dir.join("GET/http/localhost/index.html");
    if !html_path.exists() {
        anyhow::bail!("HTML file not found: {:?}", html_path);
    }
    let html_content = fs::read_to_string(&html_path)?;
    let html_lines = count_lines(&html_content);
    let minified_html_lines = count_lines(MINIFIED_HTML);

    info!("HTML:");
    info!("  Minified lines: {}", minified_html_lines);
    info!("  Beautified lines: {}", html_lines);
    info!("  Ratio: {:.2}x", html_lines as f64 / minified_html_lines as f64);

    // HTML should have significantly more lines after beautification
    if html_lines < minified_html_lines * 2 {
        anyhow::bail!(
            "HTML was not properly beautified: {} lines vs {} lines (expected at least 2x)",
            html_lines,
            minified_html_lines
        );
    }

    // Check CSS
    let css_path = contents_dir.join("GET/http/localhost/style.css");
    if !css_path.exists() {
        anyhow::bail!("CSS file not found: {:?}", css_path);
    }
    let css_content = fs::read_to_string(&css_path)?;
    let css_lines = count_lines(&css_content);
    let minified_css_lines = count_lines(MINIFIED_CSS);

    info!("CSS:");
    info!("  Minified lines: {}", minified_css_lines);
    info!("  Beautified lines: {}", css_lines);
    info!("  Ratio: {:.2}x", css_lines as f64 / minified_css_lines as f64);

    // CSS should have significantly more lines after beautification
    if css_lines < minified_css_lines * 2 {
        anyhow::bail!(
            "CSS was not properly beautified: {} lines vs {} lines (expected at least 2x)",
            css_lines,
            minified_css_lines
        );
    }

    // Check JavaScript
    let js_path = contents_dir.join("GET/http/localhost/script.js");
    if !js_path.exists() {
        anyhow::bail!("JavaScript file not found: {:?}", js_path);
    }
    let js_content = fs::read_to_string(&js_path)?;
    let js_lines = count_lines(&js_content);
    let minified_js_lines = count_lines(MINIFIED_JS);

    info!("JavaScript:");
    info!("  Minified lines: {}", minified_js_lines);
    info!("  Beautified lines: {}", js_lines);
    info!("  Ratio: {:.2}x", js_lines as f64 / minified_js_lines as f64);

    // JavaScript should have significantly more lines after beautification
    if js_lines < minified_js_lines * 2 {
        anyhow::bail!(
            "JavaScript was not properly beautified: {} lines vs {} lines (expected at least 2x)",
            js_lines,
            minified_js_lines
        );
    }

    info!("\nAll content files were properly beautified!");
    Ok(())
}

// Verify inventory has minify flags
fn verify_inventory_minify_flags(inventory_dir: &PathBuf) -> Result<()> {
    info!("\n--- Verifying Inventory Minify Flags ---");

    let inventory_path = inventory_dir.join("inventory.json");
    let inventory_json = fs::read_to_string(&inventory_path)?;
    let inventory: Inventory = serde_json::from_str(&inventory_json)?;

    info!(
        "Verifying inventory with {} resources",
        inventory.resources.len()
    );

    // We expect 3 resources: HTML, CSS, JS
    if inventory.resources.len() < 3 {
        anyhow::bail!(
            "Expected at least 3 resources in inventory, found {}",
            inventory.resources.len()
        );
    }

    let mut html_found = false;
    let mut css_found = false;
    let mut js_found = false;

    for resource in &inventory.resources {
        info!("Resource: {} {}", resource.method, resource.url);
        info!("  minify: {:?}", resource.minify);

        // Check if this is one of our test resources
        if resource.url.ends_with("/") || resource.url.ends_with("/index.html") {
            if resource.minify != Some(true) {
                anyhow::bail!("HTML resource should have minify: true, got: {:?}", resource.minify);
            }
            html_found = true;
        } else if resource.url.ends_with("/style.css") {
            if resource.minify != Some(true) {
                anyhow::bail!("CSS resource should have minify: true, got: {:?}", resource.minify);
            }
            css_found = true;
        } else if resource.url.ends_with("/script.js") {
            if resource.minify != Some(true) {
                anyhow::bail!("JS resource should have minify: true, got: {:?}", resource.minify);
            }
            js_found = true;
        }
    }

    if !html_found {
        anyhow::bail!("HTML resource not found in inventory");
    }
    if !css_found {
        anyhow::bail!("CSS resource not found in inventory");
    }
    if !js_found {
        anyhow::bail!("JavaScript resource not found in inventory");
    }

    info!("All resources have correct minify flags!");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== Content Beautification Acceptance Test ===");
    info!("Testing that minified HTML/CSS/JS are properly beautified during recording");

    let mock_server_port = 18080;
    let recording_proxy_port = 18081;

    // Start mock HTTP server
    info!("\nStarting mock HTTP server on port {}", mock_server_port);
    tokio::spawn(async move {
        if let Err(e) = start_mock_server(mock_server_port).await {
            error!("Mock server error: {:?}", e);
        }
    });

    // Wait for server to start
    sleep(Duration::from_secs(2)).await;

    // Create temporary inventory directory
    let temp_dir = tempfile::tempdir()?;
    let inventory_dir = temp_dir.path().to_path_buf();
    info!("Using inventory directory: {:?}", inventory_dir);

    // === Phase 1: Recording ===
    info!("\n--- Phase 1: Recording ---");

    let entry_url = format!("http://localhost:{}/", mock_server_port);
    let mut recording_proxy =
        start_recording_proxy(&entry_url, recording_proxy_port, &inventory_dir)?;

    // Wait for proxy to start
    sleep(Duration::from_secs(2)).await;

    // Make requests for HTML, CSS, and JS
    info!("Making request for HTML");
    make_request(recording_proxy_port, &format!("http://localhost:{}/", mock_server_port)).await?;

    info!("Making request for CSS");
    make_request(recording_proxy_port, &format!("http://localhost:{}/style.css", mock_server_port)).await?;

    info!("Making request for JavaScript");
    make_request(recording_proxy_port, &format!("http://localhost:{}/script.js", mock_server_port)).await?;

    info!("All requests completed");

    // Send SIGINT to recording proxy for graceful shutdown
    info!("\nStopping recording proxy");
    unsafe {
        libc::kill(recording_proxy.id() as i32, libc::SIGINT);
    }

    // Wait for graceful shutdown
    sleep(Duration::from_secs(3)).await;

    // Force kill if still running
    let _ = recording_proxy.kill();
    let _ = recording_proxy.wait();

    // === Phase 2: Verification ===
    info!("\n--- Phase 2: Verification ---");

    // Verify that content was beautified
    verify_beautified_content(&inventory_dir)?;

    // Verify that inventory has minify flags
    verify_inventory_minify_flags(&inventory_dir)?;

    info!("\n=================================");
    info!("  CONTENT TEST PASSED!");
    info!("=================================");

    Ok(())
}
