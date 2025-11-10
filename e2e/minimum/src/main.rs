use anyhow::{Context, Result};
use bytes::Bytes;
use futures::stream;
use http::{Request, Response, StatusCode};
use http_body_util::{combinators::BoxBody, BodyExt, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::time::sleep;
use tracing::{error, info};

// Test scenario configuration
#[derive(Debug, Clone)]
struct TestScenario {
    name: String,
    ttfb_ms: u64,
    transfer_duration_ms: u64,
    file_size: usize,
}

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
    #[serde(rename = "ttfbMs")]
    ttfb_ms: Option<u64>,
    #[serde(rename = "downloadEndMs")]
    download_end_ms: Option<u64>,
    mbps: Option<f64>,
}

// Timing measurement
#[derive(Debug)]
struct TimingMeasurement {
    ttfb_ms: u64,
    total_ms: u64,
}

fn test_scenarios() -> Vec<TestScenario> {
    vec![
        // 500KB scenarios
        TestScenario {
            name: "500kb-fast".to_string(),
            ttfb_ms: 100,
            transfer_duration_ms: 200,
            file_size: 500 * 1024,
        },
        TestScenario {
            name: "500kb-medium".to_string(),
            ttfb_ms: 500,
            transfer_duration_ms: 1000,
            file_size: 500 * 1024,
        },
        TestScenario {
            name: "500kb-slow".to_string(),
            ttfb_ms: 1000,
            transfer_duration_ms: 2000,
            file_size: 500 * 1024,
        },
        // 1KB scenarios (longer transfer times to account for system overhead)
        TestScenario {
            name: "1kb-fast".to_string(),
            ttfb_ms: 100,
            transfer_duration_ms: 100,
            file_size: 1024,
        },
        TestScenario {
            name: "1kb-medium".to_string(),
            ttfb_ms: 500,
            transfer_duration_ms: 200,
            file_size: 1024,
        },
        TestScenario {
            name: "1kb-slow".to_string(),
            ttfb_ms: 1000,
            transfer_duration_ms: 400,
            file_size: 1024,
        },
    ]
}

// Mock HTTP server handler
async fn handle_request(
    req: Request<Incoming>,
    scenario: Arc<TestScenario>,
) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    let path = req.uri().path();
    info!("Mock server received request for: {}", path);

    // Wait for TTFB
    sleep(Duration::from_millis(scenario.ttfb_ms)).await;

    // After TTFB wait, we immediately return response headers
    // This is when the client will measure TTFB

    // Calculate chunk size to achieve target transfer duration
    let file_size = scenario.file_size;
    let chunk_size = if scenario.transfer_duration_ms > 0 {
        // Divide into 10 chunks for smooth transfer simulation
        file_size / 10
    } else {
        file_size
    };

    let num_chunks = if chunk_size > 0 {
        (file_size + chunk_size - 1) / chunk_size
    } else {
        1
    };

    let delay_per_chunk = if num_chunks > 1 && scenario.transfer_duration_ms > 0 {
        scenario.transfer_duration_ms / num_chunks as u64
    } else {
        0
    };

    info!("Mock server: TTFB={}ms, streaming {} bytes in {} chunks with {}ms delay per chunk",
          scenario.ttfb_ms, file_size, num_chunks, delay_per_chunk);

    // Create a stream that sends chunks with delays
    let stream = stream::unfold(
        (0usize, chunk_size, delay_per_chunk, file_size),
        move |(sent, chunk_size, delay, file_size)| async move {
            if sent >= file_size {
                return None;
            }

            // Wait before sending this chunk (except for the first chunk)
            if sent > 0 && delay > 0 {
                sleep(Duration::from_millis(delay)).await;
            }

            // Calculate how much to send in this chunk
            let remaining = file_size - sent;
            let this_chunk_size = remaining.min(chunk_size);

            // Create chunk data
            let chunk_data = vec![b'X'; this_chunk_size];
            let frame = Frame::data(Bytes::from(chunk_data));

            Some((Ok::<_, std::io::Error>(frame), (sent + this_chunk_size, chunk_size, delay, file_size)))
        },
    );

    let body = StreamBody::new(stream).boxed();

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Length", file_size.to_string())
        .body(body)?;

    Ok(response)
}

// Start mock HTTP server
async fn start_mock_server(port: u16, scenario: Arc<TestScenario>) -> Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;

    info!(
        "Mock HTTP server listening on http://{} (scenario: {})",
        addr, scenario.name
    );

    loop {
        let (stream, _) = listener.accept().await?;
        let scenario = scenario.clone();

        tokio::spawn(async move {
            let service = service_fn(move |req| {
                let scenario = scenario.clone();
                handle_request(req, scenario)
            });

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
    // Use CARGO_MANIFEST_DIR to get workspace root
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("failed to resolve workspace root")?;

    let manifest_path = repo_root.join("Cargo.toml");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());

    #[cfg(windows)]
    let child = {
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
            .arg("--inventory")
            .arg(inventory_dir.to_str().unwrap())
            .creation_flags(CREATE_NEW_PROCESS_GROUP)
            .spawn()?
    };

    #[cfg(not(windows))]
    let child = Command::new(cargo)
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
        .arg("--inventory")
        .arg(inventory_dir.to_str().unwrap())
        .spawn()?;

    Ok(child)
}

// Start playback proxy
fn start_playback_proxy(proxy_port: u16, inventory_dir: &PathBuf) -> Result<Child> {
    // Use CARGO_MANIFEST_DIR to get workspace root
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("failed to resolve workspace root")?;

    let manifest_path = repo_root.join("Cargo.toml");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());

    let child = Command::new(cargo)
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
        .arg(inventory_dir.to_str().unwrap())
        .spawn()?;

    Ok(child)
}

// Measure request timing through proxy
async fn measure_timing(proxy_port: u16, url: &str) -> Result<TimingMeasurement> {
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::http(format!(
            "http://127.0.0.1:{}",
            proxy_port
        ))?)
        .build()?;

    let start = Instant::now();
    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Request failed with status: {}", response.status());
    }

    // TTFB is measured when we get the response headers
    let ttfb_ms = start.elapsed().as_millis() as u64;

    // Read the full body
    let _body = response.bytes().await?;
    let total_ms = start.elapsed().as_millis() as u64;

    Ok(TimingMeasurement { ttfb_ms, total_ms })
}

// Verify timing within tolerance
fn verify_timing(
    measured: &TimingMeasurement,
    expected_ttfb_ms: u64,
    expected_total_ms: u64,
    tolerance: f64,
) -> Result<()> {
    let ttfb_diff_ratio =
        ((measured.ttfb_ms as f64 - expected_ttfb_ms as f64).abs() / expected_ttfb_ms as f64)
            .abs();
    let total_diff_ratio =
        ((measured.total_ms as f64 - expected_total_ms as f64).abs() / expected_total_ms as f64)
            .abs();

    info!(
        "  TTFB: measured={}ms, expected={}ms, diff={:.1}%",
        measured.ttfb_ms,
        expected_ttfb_ms,
        ttfb_diff_ratio * 100.0
    );
    info!(
        "  Total: measured={}ms, expected={}ms, diff={:.1}%",
        measured.total_ms,
        expected_total_ms,
        total_diff_ratio * 100.0
    );

    if ttfb_diff_ratio > tolerance {
        anyhow::bail!(
            "TTFB timing outside tolerance: measured={}ms, expected={}ms, diff={:.1}%",
            measured.ttfb_ms,
            expected_ttfb_ms,
            ttfb_diff_ratio * 100.0
        );
    }

    if total_diff_ratio > tolerance {
        anyhow::bail!(
            "Total timing outside tolerance: measured={}ms, expected={}ms, diff={:.1}%",
            measured.total_ms,
            expected_total_ms,
            total_diff_ratio * 100.0
        );
    }

    Ok(())
}

// Read and verify inventory
fn verify_inventory(
    inventory_dir: &PathBuf,
    scenario: &TestScenario,
    tolerance: f64,
) -> Result<()> {
    let inventory_path = inventory_dir.join("index.json");
    let inventory_json = fs::read_to_string(&inventory_path)?;
    let inventory: Inventory = serde_json::from_str(&inventory_json)?;

    info!(
        "Verifying inventory with {} resources",
        inventory.resources.len()
    );

    if inventory.resources.is_empty() {
        anyhow::bail!("No resources found in inventory");
    }

    let resource = &inventory.resources[0];
    let expected_ttfb_ms = scenario.ttfb_ms;
    let expected_transfer_duration_ms = scenario.transfer_duration_ms;

    let recorded_ttfb_ms = resource.ttfb_ms.unwrap_or(0);
    let recorded_download_end_ms = resource.download_end_ms.unwrap_or(0);
    let recorded_transfer_duration_ms = recorded_download_end_ms.saturating_sub(recorded_ttfb_ms);

    info!(
        "Recording verification for scenario '{}':",
        scenario.name
    );
    info!(
        "  TTFB: recorded={}ms, expected={}ms",
        recorded_ttfb_ms, expected_ttfb_ms
    );
    info!(
        "  Transfer duration: recorded={}ms, expected={}ms",
        recorded_transfer_duration_ms, expected_transfer_duration_ms
    );

    // Verify TTFB
    let ttfb_diff_ratio = ((recorded_ttfb_ms as f64 - expected_ttfb_ms as f64).abs()
        / expected_ttfb_ms as f64)
        .abs();

    if ttfb_diff_ratio > tolerance {
        anyhow::bail!(
            "Recorded TTFB outside tolerance: recorded={}ms, expected={}ms, diff={:.1}%",
            recorded_ttfb_ms,
            expected_ttfb_ms,
            ttfb_diff_ratio * 100.0
        );
    }

    // Verify transfer duration
    let transfer_diff_ratio = if expected_transfer_duration_ms > 0 {
        ((recorded_transfer_duration_ms as f64 - expected_transfer_duration_ms as f64).abs()
            / expected_transfer_duration_ms as f64)
            .abs()
    } else {
        if recorded_transfer_duration_ms < 100 {
            0.0
        } else {
            1.0
        }
    };

    if transfer_diff_ratio > tolerance {
        anyhow::bail!(
            "Recorded transfer duration outside tolerance: recorded={}ms, expected={}ms, diff={:.1}%",
            recorded_transfer_duration_ms,
            expected_transfer_duration_ms,
            transfer_diff_ratio * 100.0
        );
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== Minimum Timing Acceptance Test ===");
    info!("Testing files with various sizes, latencies and transfer speeds");

    let tolerance = 0.10; // 10% tolerance

    // Test all scenarios
    for (idx, scenario) in test_scenarios().iter().enumerate() {
        info!("\n=== Testing scenario: {} ({} bytes, TTFB={}ms, transfer={}ms) ===",
              scenario.name, scenario.file_size, scenario.ttfb_ms, scenario.transfer_duration_ms);

        // Use different ports for each scenario to avoid conflicts
        let base_port = 17080 + (idx as u16 * 10);
        let mock_server_port = base_port;
        let recording_proxy_port = base_port + 1;
        let playback_proxy_port = base_port + 2;

        let scenario = Arc::new(scenario.clone());

        // Start mock HTTP server
        info!("Starting mock HTTP server on port {}", mock_server_port);
        let server_scenario = scenario.clone();
        tokio::spawn(async move {
            if let Err(e) = start_mock_server(mock_server_port, server_scenario).await {
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

        let entry_url = format!("http://localhost:{}/test", mock_server_port);
        let mut recording_proxy =
            start_recording_proxy(&entry_url, recording_proxy_port, &inventory_dir)?;

        // Wait for proxy to start
        sleep(Duration::from_secs(2)).await;

        // Make request through recording proxy
        info!("Making request through recording proxy");
        let url = format!("http://localhost:{}/test", mock_server_port);
        let recording_timing = measure_timing(recording_proxy_port, &url).await?;

        info!("Recording completed:");
        info!("  TTFB: {}ms", recording_timing.ttfb_ms);
        info!("  Total: {}ms", recording_timing.total_ms);

        // Send SIGINT to recording proxy for graceful shutdown
        info!("Stopping recording proxy");
        #[cfg(unix)]
        {
            unsafe {
                libc::kill(recording_proxy.id() as i32, libc::SIGINT);
            }
            // Wait for graceful shutdown
            sleep(Duration::from_secs(3)).await;
            let _ = recording_proxy.wait();
        }
        #[cfg(windows)]
        {
            // Windows: Send Ctrl+Break event for graceful shutdown
            const CTRL_BREAK_EVENT: u32 = 1;

            unsafe {
                #[link(name = "kernel32")]
                extern "system" {
                    fn GenerateConsoleCtrlEvent(dwCtrlEvent: u32, dwProcessGroupId: u32) -> i32;
                }

                let result = GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, recording_proxy.id());
                if result != 0 {
                    info!("Sent Ctrl+Break event, waiting for graceful shutdown");
                    sleep(Duration::from_secs(3)).await;
                    let _ = recording_proxy.wait();
                } else {
                    info!("Failed to send Ctrl+Break, force killing");
                    let _ = recording_proxy.kill();
                    let _ = recording_proxy.wait();
                }
            }
        }

        // Verify inventory
        info!("\n--- Verifying Inventory ---");
        verify_inventory(&inventory_dir, &scenario, tolerance)?;
        info!("Inventory verification PASSED");

        // Read recorded values from inventory for playback verification
        let inventory_path = inventory_dir.join("index.json");
        let inventory_json = fs::read_to_string(&inventory_path)?;
        let inventory: Inventory = serde_json::from_str(&inventory_json)?;
        let recorded_resource = &inventory.resources[0];
        let recorded_ttfb_ms = recorded_resource.ttfb_ms.unwrap_or(0);
        let recorded_download_end_ms = recorded_resource.download_end_ms.unwrap_or(0);
        let recorded_total_ms = recorded_download_end_ms;

        // === Phase 2: Playback ===
        info!("\n--- Phase 2: Playback ---");

        let mut playback_proxy = start_playback_proxy(playback_proxy_port, &inventory_dir)?;

        // Wait for proxy to start
        sleep(Duration::from_secs(2)).await;

        // Make request through playback proxy
        info!("Making request through playback proxy");
        let playback_timing = measure_timing(playback_proxy_port, &url).await?;

        info!("Playback completed:");
        info!("  TTFB: {}ms", playback_timing.ttfb_ms);
        info!("  Total: {}ms", playback_timing.total_ms);

        // Verify playback timing against RECORDED values (not scenario values)
        info!("\n--- Verifying Playback Timing ---");
        verify_timing(
            &playback_timing,
            recorded_ttfb_ms,
            recorded_total_ms,
            tolerance,
        )?;
        info!("Playback timing verification PASSED");

        // Cleanup
        let _ = playback_proxy.kill();
        let _ = playback_proxy.wait();

        info!("\n=== Scenario '{}' PASSED ===\n", scenario.name);
    }

    info!("\n=================================");
    info!("  ALL TESTS PASSED!");
    info!("=================================");

    Ok(())
}
