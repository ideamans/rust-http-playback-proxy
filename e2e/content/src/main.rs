use anyhow::{Context, Result};
use bytes::Bytes;
use encoding_rs::{Encoding, SHIFT_JIS, EUC_JP, UTF_8};
use flate2::write::{GzEncoder, DeflateEncoder};
use flate2::Compression;
use http::{Request, Response, StatusCode};
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Incoming;
use std::convert::Infallible;
use std::io::Write;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::time::sleep;
use tracing::{error, info};

// Minified test content
const MINIFIED_HTML: &str = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Test</title><link rel="stylesheet" href="/style.css"></head><body><div class="container"><h1>Test Page</h1><p>This is a test page with minified content.</p><ul><li>Item 1</li><li>Item 2</li><li>Item 3</li></ul></div><script src="/script.js"></script></body></html>"#;

const MINIFIED_CSS: &str = r#"body{margin:0;padding:0;font-family:Arial,sans-serif}.container{max-width:1200px;margin:0 auto;padding:20px}h1{color:#333;font-size:2em;margin-bottom:1em}p{line-height:1.6;color:#666}ul{list-style:none;padding:0}li{padding:10px;margin:5px 0;background:#f0f0f0;border-radius:5px}"#;

const MINIFIED_JS: &str = r#"(function(){"use strict";function init(){console.log("Initialized");document.addEventListener("DOMContentLoaded",function(){var items=document.querySelectorAll("li");items.forEach(function(item,index){item.addEventListener("click",function(){alert("Clicked item "+(index+1))})})});}init();})();"#;

// Charset test content - HTML with different charsets
const HTML_SHIFT_JIS_META: &str = r#"<!DOCTYPE html><html><head><meta charset="Shift_JIS"><title>テスト</title></head><body><h1>Shift_JISのテスト</h1><p>これはShift_JISでエンコードされたHTMLです。</p></body></html>"#;

const HTML_EUC_JP_META: &str = r#"<!DOCTYPE html><html><head><meta charset="EUC-JP"><title>テスト</title></head><body><h1>EUC-JPのテスト</h1><p>これはEUC-JPでエンコードされたHTMLです。</p></body></html>"#;

const HTML_UTF8_META: &str = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><title>テスト</title></head><body><h1>UTF-8のテスト</h1><p>これはUTF-8でエンコードされたHTMLです。</p></body></html>"#;

// CSS with different charsets
const CSS_SHIFT_JIS: &str = r#"@charset "Shift_JIS";body{margin:0;padding:0}/* 日本語コメント */.test{color:red}"#;

const CSS_EUC_JP: &str = r#"@charset "EUC-JP";body{margin:0;padding:0}/* 日本語コメント */.test{color:blue}"#;

const CSS_UTF8: &str = r#"@charset "UTF-8";body{margin:0;padding:0}/* 日本語コメント */.test{color:green}"#;

// JavaScript with different charsets (in comment)
const JS_SHIFT_JIS: &str = r#"/* Shift_JIS エンコード */ console.log("日本語");"#;

const JS_EUC_JP: &str = r#"/* EUC-JP エンコード */ console.log("日本語");"#;

const JS_UTF8: &str = r#"/* UTF-8 エンコード */ console.log("日本語");"#;

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
    #[serde(rename = "contentCharset", skip_serializing_if = "Option::is_none")]
    content_charset: Option<String>,
    #[serde(rename = "contentEncoding", skip_serializing_if = "Option::is_none")]
    content_encoding: Option<String>,
    #[serde(rename = "contentFilePath", skip_serializing_if = "Option::is_none")]
    content_file_path: Option<String>,
}

// Helper functions for charset encoding
fn encode_to_charset(content: &str, encoding: &'static Encoding) -> Vec<u8> {
    let (encoded, _, _) = encoding.encode(content);
    encoded.into_owned()
}

// Helper functions for content encoding
fn encode_gzip(content: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(content).unwrap();
    encoder.finish().unwrap()
}

fn encode_deflate(content: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(content).unwrap();
    encoder.finish().unwrap()
}

fn encode_brotli(content: &[u8]) -> Vec<u8> {
    let mut compressed = Vec::new();
    brotli::BrotliCompress(
        &mut std::io::Cursor::new(content),
        &mut compressed,
        &Default::default(),
    ).unwrap();
    compressed
}

// Mock HTTP server handler
async fn handle_request(
    req: Request<Incoming>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
    let path = req.uri().path();
    info!("Mock server received request for: {}", path);

    // Original minify tests
    if path == "/" || path == "/index.html" {
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(Full::new(Bytes::from(MINIFIED_HTML)).boxed())
            .unwrap();
        return Ok(response);
    }

    if path == "/style.css" {
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/css; charset=utf-8")
            .body(Full::new(Bytes::from(MINIFIED_CSS)).boxed())
            .unwrap();
        return Ok(response);
    }

    if path == "/script.js" {
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/javascript; charset=utf-8")
            .body(Full::new(Bytes::from(MINIFIED_JS)).boxed())
            .unwrap();
        return Ok(response);
    }

    // === Charset tests ===
    // HTML with Shift_JIS
    if path == "/charset/html-shiftjis.html" {
        let body = encode_to_charset(HTML_SHIFT_JIS_META, SHIFT_JIS);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html; charset=Shift_JIS")
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // HTML with EUC-JP
    if path == "/charset/html-eucjp.html" {
        let body = encode_to_charset(HTML_EUC_JP_META, EUC_JP);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html; charset=EUC-JP")
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // HTML with UTF-8
    if path == "/charset/html-utf8.html" {
        let body = encode_to_charset(HTML_UTF8_META, UTF_8);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html; charset=UTF-8")
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // CSS with Shift_JIS
    if path == "/charset/style-shiftjis.css" {
        let body = encode_to_charset(CSS_SHIFT_JIS, SHIFT_JIS);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/css; charset=Shift_JIS")
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // CSS with EUC-JP
    if path == "/charset/style-eucjp.css" {
        let body = encode_to_charset(CSS_EUC_JP, EUC_JP);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/css; charset=EUC-JP")
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // CSS with UTF-8
    if path == "/charset/style-utf8.css" {
        let body = encode_to_charset(CSS_UTF8, UTF_8);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/css; charset=UTF-8")
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // JavaScript with Shift_JIS
    if path == "/charset/script-shiftjis.js" {
        let body = encode_to_charset(JS_SHIFT_JIS, SHIFT_JIS);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/javascript; charset=Shift_JIS")
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // JavaScript with EUC-JP
    if path == "/charset/script-eucjp.js" {
        let body = encode_to_charset(JS_EUC_JP, EUC_JP);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/javascript; charset=EUC-JP")
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // JavaScript with UTF-8
    if path == "/charset/script-utf8.js" {
        let body = encode_to_charset(JS_UTF8, UTF_8);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/javascript; charset=UTF-8")
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // === Content-Encoding tests ===
    // Gzip-encoded HTML
    if path == "/encoding/gzip.html" {
        let plain = encode_to_charset(HTML_UTF8_META, UTF_8);
        let body = encode_gzip(&plain);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html; charset=UTF-8")
            .header("Content-Encoding", "gzip")
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // Brotli-encoded HTML
    if path == "/encoding/br.html" {
        let plain = encode_to_charset(HTML_UTF8_META, UTF_8);
        let body = encode_brotli(&plain);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html; charset=UTF-8")
            .header("Content-Encoding", "br")
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // Deflate-encoded HTML
    if path == "/encoding/deflate.html" {
        let plain = encode_to_charset(HTML_UTF8_META, UTF_8);
        let body = encode_deflate(&plain);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html; charset=UTF-8")
            .header("Content-Encoding", "deflate")
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // === Combination tests ===
    // Shift_JIS + Gzip
    if path == "/combo/shiftjis-gzip.html" {
        let plain = encode_to_charset(HTML_SHIFT_JIS_META, SHIFT_JIS);
        let body = encode_gzip(&plain);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html; charset=Shift_JIS")
            .header("Content-Encoding", "gzip")
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // EUC-JP + Brotli
    if path == "/combo/eucjp-br.html" {
        let plain = encode_to_charset(HTML_EUC_JP_META, EUC_JP);
        let body = encode_brotli(&plain);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html; charset=EUC-JP")
            .header("Content-Encoding", "br")
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // === Charset from content tests (no HTTP header charset) ===
    // HTML with Shift_JIS (no charset in HTTP header)
    if path == "/charset-from-content/html-shiftjis.html" {
        let body = encode_to_charset(HTML_SHIFT_JIS_META, SHIFT_JIS);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html")  // No charset in header
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // HTML with EUC-JP (no charset in HTTP header)
    if path == "/charset-from-content/html-eucjp.html" {
        let body = encode_to_charset(HTML_EUC_JP_META, EUC_JP);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html")  // No charset in header
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // CSS with Shift_JIS (no charset in HTTP header)
    if path == "/charset-from-content/style-shiftjis.css" {
        let body = encode_to_charset(CSS_SHIFT_JIS, SHIFT_JIS);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/css")  // No charset in header
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // CSS with UTF-8 (no charset in HTTP header)
    if path == "/charset-from-content/style-utf8.css" {
        let body = encode_to_charset(CSS_UTF8, UTF_8);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/css")  // No charset in header
            .body(Full::new(Bytes::from(body)).boxed())
            .unwrap();
        return Ok(response);
    }

    // 404 for unknown paths
    let response = Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::new(Bytes::from("Not Found")).boxed())
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
    control_port: u16,
    inventory_dir: &PathBuf,
) -> Result<Child> {
    // Use CARGO_MANIFEST_DIR to get workspace root
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("failed to resolve workspace root")?;

    // Check if binary exists - prefer using binary over cargo run
    // This is important because cargo run doesn't properly forward SIGINT to child process
    let binary_path = repo_root.join("target/release/http-playback-proxy");
    #[cfg(windows)]
    let binary_path = binary_path.with_extension("exe");

    // Use pre-built binary if in CI or if binary exists
    let use_prebuilt = std::env::var("CI").is_ok() || binary_path.exists();

    let child = if use_prebuilt {
        // CI or Local with binary: Use pre-built binary directly
        let binary_path = repo_root.join("target/release/http-playback-proxy");
        #[cfg(windows)]
        let binary_path = binary_path.with_extension("exe");

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

            Command::new(binary_path)
                .arg("recording")
                .arg(entry_url)
                .arg("--port")
                .arg(proxy_port.to_string())
                .arg("--control-port")
                .arg(control_port.to_string())
                .arg("--inventory")
                .arg(inventory_dir.to_str().unwrap())
                .creation_flags(CREATE_NEW_PROCESS_GROUP)
                .spawn()?
        }

        #[cfg(not(windows))]
        Command::new(binary_path)
            .arg("recording")
            .arg(entry_url)
            .arg("--port")
            .arg(proxy_port.to_string())
            .arg("--control-port")
            .arg(control_port.to_string())
            .arg("--inventory")
            .arg(inventory_dir.to_str().unwrap())
            .spawn()?
    } else {
        // Local: Use cargo run for developer convenience
        let manifest_path = repo_root.join("Cargo.toml");
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

            Command::new(cargo)
                .arg("run")
                .arg("--release")
                .arg("--manifest-path")
                .arg(manifest_path)
                .arg("--bin")
                .arg("http-playback-proxy")
                .arg("--")
                .arg("recording")
                .arg(entry_url)
                .arg("--port")
                .arg(proxy_port.to_string())
                .arg("--control-port")
                .arg(control_port.to_string())
                .arg("--inventory")
                .arg(inventory_dir.to_str().unwrap())
                .creation_flags(CREATE_NEW_PROCESS_GROUP)
                .spawn()?
        }

        #[cfg(not(windows))]
        Command::new(cargo)
            .arg("run")
            .arg("--release")
            .arg("--manifest-path")
            .arg(manifest_path)
            .arg("--bin")
            .arg("http-playback-proxy")
            .arg("--")
            .arg("recording")
            .arg(entry_url)
            .arg("--port")
            .arg(proxy_port.to_string())
            .arg("--control-port")
            .arg(control_port.to_string())
            .arg("--inventory")
            .arg(inventory_dir.to_str().unwrap())
            .spawn()?
    };

    Ok(child)
}

// Make HTTP request through proxy
// Wait for proxy to be ready by checking port connectivity
async fn wait_for_proxy(port: u16, max_retries: u32) -> Result<()> {
    use tokio::net::TcpStream;
    use tokio::time::timeout;

    for i in 0..max_retries {
        // Use short timeout for connection attempt to fail fast
        match timeout(
            Duration::from_millis(500),
            TcpStream::connect(format!("127.0.0.1:{}", port)),
        )
        .await
        {
            Ok(Ok(_)) => {
                info!(
                    "Proxy is ready on port {} (took {} seconds)",
                    port,
                    i + 1
                );
                return Ok(());
            }
            Ok(Err(_)) | Err(_) => {
                if i == 0 {
                    info!("Waiting for proxy to start on port {}...", port);
                } else if (i + 1) % 10 == 0 {
                    info!("Still waiting for proxy... ({}/{} seconds)", i + 1, max_retries);
                }
            }
        }
        if i < max_retries - 1 {
            sleep(Duration::from_secs(1)).await;
        }
    }
    anyhow::bail!(
        "Proxy did not start on port {} after {} seconds. \
         This may indicate the binary is still compiling or the proxy crashed during startup.",
        port,
        max_retries
    )
}

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

// Wait for a file to exist with retry logic
async fn wait_for_file(path: &std::path::Path, max_attempts: u32) -> Result<()> {
    for attempt in 1..=max_attempts {
        if path.exists() {
            info!("File found: {:?} (attempt {})", path, attempt);
            return Ok(());
        }
        if attempt < max_attempts {
            info!("Waiting for file: {:?} (attempt {}/{})", path, attempt, max_attempts);
            sleep(Duration::from_secs(1)).await;
        }
    }
    anyhow::bail!("File not found after {} attempts: {:?}", max_attempts, path)
}

// Verify that content was beautified
async fn verify_beautified_content(inventory_dir: &PathBuf) -> Result<()> {
    info!("\n--- Verifying Beautified Content ---");

    let contents_dir = inventory_dir.join("contents");
    if !contents_dir.exists() {
        anyhow::bail!("Contents directory not found: {:?}", contents_dir);
    }

    // Check HTML - wait for file to exist (method is lowercase per generate_file_path_from_url)
    let html_path = contents_dir.join("get/http/127.0.0.1/index.html");
    wait_for_file(&html_path, 10).await?;
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

    // Check CSS - wait for file to exist (method is lowercase per generate_file_path_from_url)
    let css_path = contents_dir.join("get/http/127.0.0.1/style.css");
    wait_for_file(&css_path, 10).await?;
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

    // Check JavaScript - wait for file to exist (method is lowercase per generate_file_path_from_url)
    let js_path = contents_dir.join("get/http/127.0.0.1/script.js");
    wait_for_file(&js_path, 10).await?;
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

    let inventory_path = inventory_dir.join("index.json");
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

// Verify charset handling in inventory
fn verify_charset_in_inventory(inventory_dir: &PathBuf) -> Result<()> {
    info!("\n--- Verifying Charset Handling in Inventory ---");

    let inventory_path = inventory_dir.join("index.json");
    let inventory_json = fs::read_to_string(&inventory_path)?;
    let inventory: Inventory = serde_json::from_str(&inventory_json)?;

    info!("Checking {} resources for charset handling", inventory.resources.len());

    for resource in &inventory.resources {
        let url = &resource.url;

        // Check charset test resources
        if url.contains("/charset/") {
            info!("\nCharset resource: {}", url);
            info!("  contentCharset: {:?}", resource.content_charset);

            if url.contains("-shiftjis.") {
                if resource.content_charset != Some("Shift_JIS".to_string()) {
                    anyhow::bail!(
                        "Shift_JIS resource should have contentCharset=Shift_JIS, got: {:?}",
                        resource.content_charset
                    );
                }
                info!("  ✓ Shift_JIS charset preserved");
            } else if url.contains("-eucjp.") {
                if resource.content_charset != Some("EUC-JP".to_string()) {
                    anyhow::bail!(
                        "EUC-JP resource should have contentCharset=EUC-JP, got: {:?}",
                        resource.content_charset
                    );
                }
                info!("  ✓ EUC-JP charset preserved");
            } else if url.contains("-utf8.") {
                if resource.content_charset != Some("UTF-8".to_string()) {
                    anyhow::bail!(
                        "UTF-8 resource should have contentCharset=UTF-8, got: {:?}",
                        resource.content_charset
                    );
                }
                info!("  ✓ UTF-8 charset preserved");
            }

            // Verify content file is UTF-8
            if let Some(content_file_path) = &resource.content_file_path {
                let full_path = inventory_dir.join(content_file_path);
                if full_path.exists() {
                    let content = fs::read_to_string(&full_path)?;
                    // If we can read it as UTF-8 string, it's stored as UTF-8
                    info!("  ✓ Content file stored as UTF-8: {} bytes", content.len());
                }
            }
        }

        // Check encoding test resources
        if url.contains("/encoding/") || url.contains("/combo/") {
            info!("\nEncoding resource: {}", url);
            info!("  contentEncoding: {:?}", resource.content_encoding);

            if url.contains("gzip") {
                if resource.content_encoding != Some("gzip".to_string()) {
                    anyhow::bail!(
                        "Gzip resource should have contentEncoding=gzip, got: {:?}",
                        resource.content_encoding
                    );
                }
                info!("  ✓ Gzip encoding preserved");
            } else if url.contains("/br.") {
                if resource.content_encoding != Some("br".to_string()) {
                    anyhow::bail!(
                        "Brotli resource should have contentEncoding=br, got: {:?}",
                        resource.content_encoding
                    );
                }
                info!("  ✓ Brotli encoding preserved");
            } else if url.contains("deflate") {
                if resource.content_encoding != Some("deflate".to_string()) {
                    anyhow::bail!(
                        "Deflate resource should have contentEncoding=deflate, got: {:?}",
                        resource.content_encoding
                    );
                }
                info!("  ✓ Deflate encoding preserved");
            }
        }

        // Check charset-from-content test resources (charset detected from HTML/CSS content)
        if url.contains("/charset-from-content/") {
            info!("\nCharset-from-content resource: {}", url);
            info!("  contentCharset: {:?}", resource.content_charset);

            if url.contains("-shiftjis.") {
                if resource.content_charset != Some("shift_jis".to_string()) {
                    anyhow::bail!(
                        "Shift_JIS resource (detected from content) should have contentCharset=shift_jis, got: {:?}",
                        resource.content_charset
                    );
                }
                info!("  ✓ Shift_JIS charset detected from content");
            } else if url.contains("-eucjp.") {
                if resource.content_charset != Some("euc-jp".to_string()) {
                    anyhow::bail!(
                        "EUC-JP resource (detected from content) should have contentCharset=euc-jp, got: {:?}",
                        resource.content_charset
                    );
                }
                info!("  ✓ EUC-JP charset detected from content");
            } else if url.contains("-utf8.") {
                if resource.content_charset != Some("utf-8".to_string()) {
                    anyhow::bail!(
                        "UTF-8 resource (detected from content) should have contentCharset=utf-8, got: {:?}",
                        resource.content_charset
                    );
                }
                info!("  ✓ UTF-8 charset detected from content");
            }

            // Verify content file is UTF-8
            if let Some(content_file_path) = &resource.content_file_path {
                let full_path = inventory_dir.join(content_file_path);
                if full_path.exists() {
                    let content = fs::read_to_string(&full_path)?;
                    // If we can read it as UTF-8 string, it's stored as UTF-8
                    info!("  ✓ Content file stored as UTF-8: {} bytes", content.len());
                }
            }
        }
    }

    info!("\nAll charset and encoding metadata verified!");
    Ok(())
}

// Start playback proxy
fn start_playback_proxy(proxy_port: u16, inventory_dir: &PathBuf) -> Result<Child> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("failed to resolve workspace root")?;

    // Convert inventory_dir to absolute path to ensure child process can access it
    let absolute_inventory_dir = if inventory_dir.is_absolute() {
        inventory_dir.clone()
    } else {
        std::env::current_dir()?.join(inventory_dir)
    };

    info!("start_playback_proxy: inventory_dir = {:?}", inventory_dir);
    info!("start_playback_proxy: absolute_inventory_dir = {:?}", absolute_inventory_dir);

    // Check if binary exists - prefer using binary over cargo run
    // This is important because cargo run doesn't properly forward SIGINT to child process
    let binary_path = repo_root.join("target/release/http-playback-proxy");
    #[cfg(windows)]
    let binary_path = binary_path.with_extension("exe");

    // Use pre-built binary if in CI or if binary exists
    let use_prebuilt = std::env::var("CI").is_ok() || binary_path.exists();

    let child = if use_prebuilt {
        // CI or Local with binary: Use pre-built binary directly
        let binary_path = repo_root.join("target/release/http-playback-proxy");
        #[cfg(windows)]
        let binary_path = binary_path.with_extension("exe");

        Command::new(binary_path)
            .arg("playback")
            .arg("--port")
            .arg(proxy_port.to_string())
            .arg("--inventory")
            .arg(absolute_inventory_dir.to_str().unwrap())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()?
    } else {
        // Local: Use cargo run
        let manifest_path = repo_root.join("Cargo.toml");
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());

        Command::new(cargo)
            .arg("run")
            .arg("--release")
            .arg("--manifest-path")
            .arg(manifest_path)
            .arg("--bin")
            .arg("http-playback-proxy")
            .arg("--")
            .arg("playback")
            .arg("--port")
            .arg(proxy_port.to_string())
            .arg("--inventory")
            .arg(absolute_inventory_dir.to_str().unwrap())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()?
    };

    Ok(child)
}

// Verify playback reproduces original charset and encoding
async fn verify_playback_proxy(
    inventory_dir: &PathBuf,
    playback_proxy_port: u16,
    mock_server_host: &str,
    mock_server_port: u16,
) -> Result<()> {
    info!("\n--- Verifying Playback Charset/Encoding Reproduction ---");
    info!("Inventory directory: {:?}", inventory_dir);
    info!("Playback proxy port: {}", playback_proxy_port);

    // Double-check inventory directory exists before starting playback proxy
    info!("Pre-check: inventory_dir exists? {}", inventory_dir.exists());
    if inventory_dir.exists() {
        info!("Pre-check: inventory_dir is a directory? {}", inventory_dir.is_dir());
        info!("Pre-check: inventory_dir path: {:?}", inventory_dir);

        // List files
        if let Ok(entries) = std::fs::read_dir(inventory_dir) {
            for entry in entries {
                if let Ok(entry) = entry {
                    info!("  Pre-check file: {:?}", entry.path());
                }
            }
        }
    }

    // Start playback proxy
    let mut playback_proxy = start_playback_proxy(playback_proxy_port, inventory_dir)?;
    info!("Playback proxy started with PID: {}", playback_proxy.id());

    // Give child process time to actually start before waiting for port
    sleep(Duration::from_millis(500)).await;
    info!("After 500ms sleep, checking if playback proxy process is still alive...");
    match playback_proxy.try_wait()? {
        Some(status) => {
            anyhow::bail!("Playback proxy exited immediately with status: {:?}", status);
        }
        None => {
            info!("✓ Playback proxy process is still running");
        }
    }

    // Wait for playback proxy to start
    info!("Waiting for playback proxy to be ready...");
    wait_for_proxy(playback_proxy_port, 30).await?;
    info!("✓ Playback proxy is ready");

    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::http(format!(
            "http://127.0.0.1:{}",
            playback_proxy_port
        ))?)
        .build()?;

    // Test Shift_JIS charset reproduction
    info!("\nTesting Shift_JIS charset playback");
    let response = client
        .get(format!("http://{}:{}/charset/html-shiftjis.html", mock_server_host, mock_server_port))
        .send()
        .await?;

    let content_type = response.headers().get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !content_type.contains("Shift_JIS") {
        anyhow::bail!(
            "Playback should return Shift_JIS charset, got: {}",
            content_type
        );
    }
    info!("  ✓ Shift_JIS charset reproduced in playback");

    let body_bytes = response.bytes().await?;
    // Decode from Shift_JIS to verify it's actually Shift_JIS encoded
    let (decoded, _, had_errors) = SHIFT_JIS.decode(&body_bytes);
    if had_errors {
        anyhow::bail!("Playback body is not valid Shift_JIS");
    }
    info!("  ✓ Playback body is valid Shift_JIS: {} bytes", body_bytes.len());

    // Verify meta tag was NOT modified (should still say Shift_JIS)
    if !decoded.contains(r#"charset="Shift_JIS"#) && !decoded.contains(r#"charset="shift_jis"#) {
        anyhow::bail!("Meta tag should still contain Shift_JIS charset declaration");
    }
    info!("  ✓ Meta tag charset preserved in playback");

    // Test Gzip encoding reproduction
    info!("\nTesting Gzip encoding playback");
    let response = client
        .get(format!("http://{}:{}/encoding/gzip.html", mock_server_host, mock_server_port))
        .send()
        .await?;

    let content_encoding = response.headers().get("content-encoding")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_encoding != "gzip" {
        anyhow::bail!(
            "Playback should return gzip encoding, got: {}",
            content_encoding
        );
    }
    info!("  ✓ Gzip encoding reproduced in playback");

    // Test charset-from-content resources (charset detected from HTML/CSS, not HTTP header)
    info!("\nTesting charset-from-content HTML playback (Shift_JIS detected from <meta charset>)");
    let response = client
        .get(format!("http://{}:{}/charset-from-content/html-shiftjis.html", mock_server_host, mock_server_port))
        .send()
        .await?;

    // HTTP header should NOT have charset (since original didn't have it)
    let content_type = response.headers().get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type.contains("charset") {
        anyhow::bail!(
            "Playback should NOT add charset to HTTP header when it wasn't in original, got: {}",
            content_type
        );
    }
    info!("  ✓ HTTP header has no charset (preserved from recording)");

    let body_bytes = response.bytes().await?;
    // Body should be Shift_JIS encoded (re-encoded from UTF-8 storage)
    let (decoded, _, had_errors) = SHIFT_JIS.decode(&body_bytes);
    if had_errors {
        anyhow::bail!("Playback body is not valid Shift_JIS");
    }
    info!("  ✓ Playback body is valid Shift_JIS: {} bytes", body_bytes.len());

    // Verify <meta charset> is preserved in the HTML
    if !decoded.contains(r#"charset="Shift_JIS"#) && !decoded.contains(r#"charset="shift_jis"#) {
        anyhow::bail!("Meta tag should still contain Shift_JIS charset declaration");
    }
    info!("  ✓ <meta charset> declaration preserved in playback");

    // Test CSS charset-from-content
    info!("\nTesting charset-from-content CSS playback (Shift_JIS detected from @charset)");
    let response = client
        .get(format!("http://{}:{}/charset-from-content/style-shiftjis.css", mock_server_host, mock_server_port))
        .send()
        .await?;

    // HTTP header should NOT have charset (since original didn't have it)
    let content_type = response.headers().get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type.contains("charset") {
        anyhow::bail!(
            "Playback should NOT add charset to HTTP header when it wasn't in original, got: {}",
            content_type
        );
    }
    info!("  ✓ HTTP header has no charset (preserved from recording)");

    let body_bytes = response.bytes().await?;
    // Body should be Shift_JIS encoded (re-encoded from UTF-8 storage)
    let (decoded, _, had_errors) = SHIFT_JIS.decode(&body_bytes);
    if had_errors {
        anyhow::bail!("Playback CSS body is not valid Shift_JIS");
    }
    info!("  ✓ Playback CSS body is valid Shift_JIS: {} bytes", body_bytes.len());

    // Verify @charset declaration is preserved
    if !decoded.contains(r#"@charset "Shift_JIS"#) && !decoded.contains(r#"@charset "shift_jis"#) {
        anyhow::bail!("CSS should still contain Shift_JIS @charset declaration");
    }
    info!("  ✓ @charset declaration preserved in playback");

    // Stop playback proxy
    let _ = playback_proxy.kill();
    let _ = playback_proxy.wait();

    info!("\nPlayback verification complete!");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== Content Beautification Acceptance Test ===");
    info!("Testing that minified HTML/CSS/JS are properly beautified during recording");

    // Use 127.0.0.1 consistently to avoid IPv6/IPv4 mismatch on CI runners
    const MOCK_SERVER_HOST: &str = "127.0.0.1";
    const RECORDING_CONTROL_PORT: u16 = 18083;
    let mock_server_port = 18080;
    let recording_proxy_port = 18081;

    // Start mock HTTP server
    info!("\nStarting mock HTTP server on {}:{}", MOCK_SERVER_HOST, mock_server_port);
    let mock_server_handle = tokio::spawn(async move {
        if let Err(e) = start_mock_server(mock_server_port).await {
            error!("Mock server error: {:?}", e);
        }
    });

    // Wait for mock server to be ready
    info!("Waiting for mock server to be ready...");
    wait_for_proxy(mock_server_port, 30).await?;

    // Create temporary inventory directory
    // IMPORTANT: Keep _temp_dir alive until the end of the function
    // to prevent automatic cleanup of the temporary directory
    let _temp_dir = tempfile::tempdir()?;
    let inventory_dir = _temp_dir.path().to_path_buf();
    info!("Using inventory directory: {:?}", inventory_dir);

    // === Phase 1: Recording ===
    info!("\n--- Phase 1: Recording ---");

    let entry_url = format!("http://{}:{}/", MOCK_SERVER_HOST, mock_server_port);
    let mut recording_proxy =
        start_recording_proxy(&entry_url, recording_proxy_port, RECORDING_CONTROL_PORT, &inventory_dir)?;

    // Wait for proxy to start (with retry logic for CI environments where build takes time)
    wait_for_proxy(recording_proxy_port, 60).await?;

    // Make requests for HTML, CSS, and JS
    info!("Making request for HTML");
    make_request(recording_proxy_port, &format!("http://{}:{}/", MOCK_SERVER_HOST, mock_server_port)).await?;

    info!("Making request for CSS");
    make_request(recording_proxy_port, &format!("http://{}:{}/style.css", MOCK_SERVER_HOST, mock_server_port)).await?;

    info!("Making request for JavaScript");
    make_request(recording_proxy_port, &format!("http://{}:{}/script.js", MOCK_SERVER_HOST, mock_server_port)).await?;

    // Make requests for charset tests
    info!("\nMaking requests for charset tests");
    make_request(recording_proxy_port, &format!("http://{}:{}/charset/html-shiftjis.html", MOCK_SERVER_HOST, mock_server_port)).await?;
    make_request(recording_proxy_port, &format!("http://{}:{}/charset/html-eucjp.html", MOCK_SERVER_HOST, mock_server_port)).await?;
    make_request(recording_proxy_port, &format!("http://{}:{}/charset/html-utf8.html", MOCK_SERVER_HOST, mock_server_port)).await?;
    make_request(recording_proxy_port, &format!("http://{}:{}/charset/style-shiftjis.css", MOCK_SERVER_HOST, mock_server_port)).await?;
    make_request(recording_proxy_port, &format!("http://{}:{}/charset/style-eucjp.css", MOCK_SERVER_HOST, mock_server_port)).await?;
    make_request(recording_proxy_port, &format!("http://{}:{}/charset/style-utf8.css", MOCK_SERVER_HOST, mock_server_port)).await?;
    make_request(recording_proxy_port, &format!("http://{}:{}/charset/script-shiftjis.js", MOCK_SERVER_HOST, mock_server_port)).await?;
    make_request(recording_proxy_port, &format!("http://{}:{}/charset/script-eucjp.js", MOCK_SERVER_HOST, mock_server_port)).await?;
    make_request(recording_proxy_port, &format!("http://{}:{}/charset/script-utf8.js", MOCK_SERVER_HOST, mock_server_port)).await?;

    // Make requests for encoding tests
    info!("\nMaking requests for encoding tests");
    make_request(recording_proxy_port, &format!("http://{}:{}/encoding/gzip.html", MOCK_SERVER_HOST, mock_server_port)).await?;
    make_request(recording_proxy_port, &format!("http://{}:{}/encoding/br.html", MOCK_SERVER_HOST, mock_server_port)).await?;
    make_request(recording_proxy_port, &format!("http://{}:{}/encoding/deflate.html", MOCK_SERVER_HOST, mock_server_port)).await?;

    // Make requests for combination tests
    info!("\nMaking requests for combination tests");
    make_request(recording_proxy_port, &format!("http://{}:{}/combo/shiftjis-gzip.html", MOCK_SERVER_HOST, mock_server_port)).await?;
    make_request(recording_proxy_port, &format!("http://{}:{}/combo/eucjp-br.html", MOCK_SERVER_HOST, mock_server_port)).await?;

    // Make requests for charset-from-content tests (no HTTP header charset)
    info!("\nMaking requests for charset-from-content tests (detecting charset from HTML/CSS content)");
    make_request(recording_proxy_port, &format!("http://{}:{}/charset-from-content/html-shiftjis.html", MOCK_SERVER_HOST, mock_server_port)).await?;
    make_request(recording_proxy_port, &format!("http://{}:{}/charset-from-content/html-eucjp.html", MOCK_SERVER_HOST, mock_server_port)).await?;
    make_request(recording_proxy_port, &format!("http://{}:{}/charset-from-content/style-shiftjis.css", MOCK_SERVER_HOST, mock_server_port)).await?;
    make_request(recording_proxy_port, &format!("http://{}:{}/charset-from-content/style-utf8.css", MOCK_SERVER_HOST, mock_server_port)).await?;

    info!("\nAll requests completed");

    // Send shutdown request via control port for graceful shutdown
    info!("\nStopping recording proxy via control port");
    let shutdown_url = format!("http://127.0.0.1:{}/_shutdown", RECORDING_CONTROL_PORT);
    match reqwest::Client::new().post(&shutdown_url).send().await {
        Ok(_) => {
            info!("Shutdown request sent successfully");
        }
        Err(e) => {
            info!("Failed to send shutdown request: {:?}, falling back to SIGINT", e);
            #[cfg(unix)]
            unsafe {
                libc::kill(recording_proxy.id() as i32, libc::SIGINT);
            }
            #[cfg(windows)]
            {
                let _ = recording_proxy.kill();
            }
        }
    }

    // Wait for index.json to be created (with timeout)
    info!("Waiting for index.json to be created...");
    let index_path = inventory_dir.join("index.json");
    let max_wait = Duration::from_secs(30);
    let start = std::time::Instant::now();

    loop {
        if index_path.exists() {
            info!("✓ index.json created successfully");
            break;
        }

        if start.elapsed() > max_wait {
            anyhow::bail!(
                "Timeout waiting for index.json to be created. Recording proxy may have failed to shutdown gracefully."
            );
        }

        // Check if process is still alive
        match recording_proxy.try_wait()? {
            Some(status) => {
                if !index_path.exists() {
                    anyhow::bail!(
                        "Recording proxy exited with status {:?} but index.json was not created",
                        status
                    );
                }
                info!("✓ Recording proxy exited with status: {:?}", status);
                break;
            }
            None => {
                // Process still running, wait a bit
                sleep(Duration::from_millis(500)).await;
            }
        }
    }

    // Wait a bit more to ensure all content files are written
    info!("Waiting for content files to be written...");
    sleep(Duration::from_secs(2)).await;

    // === Phase 2: Verification ===
    info!("\n--- Phase 2: Verification ---");

    // Verify that content was beautified
    verify_beautified_content(&inventory_dir).await?;

    // Verify that inventory has minify flags
    verify_inventory_minify_flags(&inventory_dir)?;

    // Verify charset and encoding handling in inventory
    verify_charset_in_inventory(&inventory_dir)?;

    // === Phase 3: Playback Verification ===
    // IMPORTANT: We stop the mock server BEFORE starting playback proxy
    // to ensure we're testing actual playback, not fallback to original server
    info!("\n--- Phase 3: Playback (with original server stopped) ---");

    // Debug: Check inventory directory before stopping mock server
    info!("Checking inventory directory: {:?}", inventory_dir);

    // List all files in inventory directory
    match std::fs::read_dir(&inventory_dir) {
        Ok(entries) => {
            info!("Files in inventory directory:");
            for entry in entries {
                if let Ok(entry) = entry {
                    info!("  - {:?}", entry.path());
                }
            }
        }
        Err(e) => {
            anyhow::bail!("Failed to read inventory directory: {:?}", e);
        }
    }

    let index_path = inventory_dir.join("index.json");
    if !index_path.exists() {
        anyhow::bail!("index.json not found at {:?}", index_path);
    }
    info!("✓ index.json exists");

    let contents_dir = inventory_dir.join("contents");
    if !contents_dir.exists() {
        anyhow::bail!("contents directory not found at {:?}", contents_dir);
    }
    info!("✓ contents directory exists");

    info!("Stopping mock server to ensure playback proxy is actually working...");
    mock_server_handle.abort();

    // Wait a bit to ensure port is released
    sleep(Duration::from_secs(1)).await;

    // Verify mock server is actually stopped by trying to connect
    match tokio::net::TcpStream::connect(format!("{}:{}", MOCK_SERVER_HOST, mock_server_port)).await {
        Ok(_) => {
            anyhow::bail!("Mock server is still running on port {}! Cannot verify playback.", mock_server_port);
        }
        Err(_) => {
            info!("✓ Mock server stopped successfully");
        }
    }

    let playback_proxy_port = 18082;
    verify_playback_proxy(&inventory_dir, playback_proxy_port, MOCK_SERVER_HOST, mock_server_port).await?;

    info!("\n=================================");
    info!("  ALL CONTENT TESTS PASSED!");
    info!("=================================");
    info!("✓ Minify/Beautify");
    info!("✓ Charset handling (UTF-8, Shift_JIS, EUC-JP)");
    info!("✓ Content-Encoding (gzip, br, deflate)");
    info!("✓ Combination tests");
    info!("✓ Playback verification");

    // _temp_dir will be automatically dropped here, cleaning up the temporary directory
    Ok(())
}
