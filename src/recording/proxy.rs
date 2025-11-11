use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info};

use super::batch_processor::BatchProcessor;
use super::hudsucker_handler::RecordingHandler;
use crate::traits::{FileSystem, RealFileSystem, RealTimeProvider};
use crate::types::Inventory;

use hudsucker::{
    Proxy as HudsuckerProxy,
    certificate_authority::RcgenAuthority,
    rcgen::{CertificateParams, DistinguishedName, Issuer, KeyPair},
    rustls::crypto::aws_lc_rs,
};

pub async fn start_recording_proxy(
    port: u16,
    inventory: Inventory,
    inventory_dir: PathBuf,
) -> Result<()> {
    info!("Starting HTTPS MITM recording proxy on port {}", port);

    // Generate a self-signed CA certificate for MITM
    let key_pair = KeyPair::generate()?;
    let mut params = CertificateParams::new(vec!["http-playback-proxy.local".to_string()])?;
    params.is_ca = hudsucker::rcgen::IsCa::Ca(hudsucker::rcgen::BasicConstraints::Unconstrained);
    let mut dn = DistinguishedName::new();
    dn.push(
        hudsucker::rcgen::DnType::CommonName,
        "http-playback-proxy CA",
    );
    dn.push(
        hudsucker::rcgen::DnType::OrganizationName,
        "http-playback-proxy",
    );
    params.distinguished_name = dn;

    let cert = params.self_signed(&key_pair)?;
    let issuer = Issuer::from_ca_cert_pem(&cert.pem(), key_pair)?;

    let ca = RcgenAuthority::new(issuer, 1_000, aws_lc_rs::default_provider());

    // Create the recording handler
    let handler = RecordingHandler::new(inventory);
    let handler_inventory = handler.get_inventory();

    // Build the proxy with standard TLS configuration
    let crypto_provider = aws_lc_rs::default_provider();

    // Bind to the socket first to get the actual port (important when port=0)
    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::new(127, 0, 0, 1), port)).await?;
    let actual_addr = listener.local_addr()?;
    let actual_port = actual_addr.port();

    // Build the proxy
    let proxy = HudsuckerProxy::builder()
        .with_listener(listener)
        .with_ca(ca)
        .with_rustls_connector(crypto_provider)
        .with_http_handler(handler)
        .build()?;

    // Start the proxy server
    info!("HTTPS MITM Proxy listening on 127.0.0.1:{}", actual_port);
    info!("Configure your client to trust the self-signed CA certificate");
    info!("Send SIGTERM or press Ctrl+C to stop recording and save inventory");

    // Run proxy and signal handler concurrently
    let proxy_task = tokio::spawn(async move {
        if let Err(e) = proxy.start().await {
            error!("Proxy server error: {}", e);
        }
    });

    // Wait for shutdown signal
    if let Err(e) = super::signal_handler::wait_for_shutdown_signal().await {
        error!("Signal handler error: {}", e);
    }

    // Signal received, stop accepting new connections
    info!("Shutdown signal received, stopping proxy...");

    // Note: Hudsucker proxy doesn't provide graceful shutdown mechanism
    // We rely on the process termination to stop accepting connections
    // Give in-flight requests a moment to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    info!("Processing resources...");

    // Get mutable access to inventory for batch processing
    let mut inventory = handler_inventory.lock().await;

    // Batch process all resources
    let batch_processor = BatchProcessor::new(
        inventory_dir.clone(),
        Arc::new(RealFileSystem),
        Arc::new(RealTimeProvider::new()),
    );

    if let Err(e) = batch_processor.process_all(&mut inventory).await {
        error!("Failed to batch process resources: {}", e);
        return Err(e);
    }

    info!("All resources processed successfully");

    // Save inventory after processing
    info!("Saving inventory...");
    if let Err(e) = save_inventory(&inventory, &inventory_dir).await {
        error!("Failed to save inventory: {}", e);
        return Err(e);
    }

    info!(
        "Inventory saved successfully with {} resources",
        inventory.resources.len()
    );
    info!("Shutdown complete");

    // Abort proxy task
    proxy_task.abort();

    Ok(())
}

pub async fn save_inventory(inventory: &Inventory, inventory_dir: &Path) -> Result<()> {
    let file_system = Arc::new(RealFileSystem);
    save_inventory_with_fs(inventory, inventory_dir, file_system).await
}

pub async fn save_inventory_with_fs<F: FileSystem>(
    inventory: &Inventory,
    inventory_dir: &Path,
    file_system: Arc<F>,
) -> Result<()> {
    file_system.create_dir_all(inventory_dir).await?;

    let inventory_path = inventory_dir.join("index.json");
    // 2スペースインデントで整形
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    inventory.serialize(&mut ser)?;
    let inventory_json = String::from_utf8(buf)?;

    file_system
        .write_string(&inventory_path, &inventory_json)
        .await?;

    Ok(())
}
