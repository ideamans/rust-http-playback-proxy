use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, error};
use serde::Serialize;

use crate::types::Inventory;
use crate::traits::{FileSystem, RealFileSystem};
use super::hudsucker_handler::RecordingHandler;

use hudsucker::{
    certificate_authority::RcgenAuthority,
    rcgen::{CertificateParams, DistinguishedName, KeyPair, Issuer},
    rustls::crypto::aws_lc_rs,
    Proxy as HudsuckerProxy,
};

pub async fn start_recording_proxy(
    port: u16,
    inventory: Inventory,
    inventory_dir: PathBuf,
    ignore_tls_errors: bool,
) -> Result<()> {
    info!("Starting HTTPS MITM recording proxy on port {}", port);

    // Generate a self-signed CA certificate for MITM
    let key_pair = KeyPair::generate()?;
    let mut params = CertificateParams::new(vec!["http-playback-proxy.local".to_string()])?;
    params.is_ca = hudsucker::rcgen::IsCa::Ca(hudsucker::rcgen::BasicConstraints::Unconstrained);
    let mut dn = DistinguishedName::new();
    dn.push(hudsucker::rcgen::DnType::CommonName, "http-playback-proxy CA");
    dn.push(hudsucker::rcgen::DnType::OrganizationName, "http-playback-proxy");
    params.distinguished_name = dn;

    let cert = params.self_signed(&key_pair)?;
    let issuer = Issuer::from_ca_cert_pem(&cert.pem(), key_pair)?;

    let ca = RcgenAuthority::new(issuer, 1_000, aws_lc_rs::default_provider());

    // Create the recording handler
    let handler = RecordingHandler::new(inventory, inventory_dir.clone());
    let handler_inventory = handler.get_inventory();

    // Build the proxy with appropriate TLS configuration
    let crypto_provider = aws_lc_rs::default_provider();

    if ignore_tls_errors {
        info!("WARNING: TLS certificate verification is DISABLED - accepting all certificates!");
        // Note: Implementing TLS bypass for MITM requires deeper integration with hudsucker's internals
        // For now, this flag is accepted but custom certificate verification is not fully implemented
        // The client (e.g., reqwest) should use .danger_accept_invalid_certs(true)
    }

    // Bind to the socket first to get the actual port (important when port=0)
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::new(127, 0, 0, 1), port)).await?;
    let actual_addr = listener.local_addr()?;
    let actual_port = actual_addr.port();

    // Build the proxy (TLS bypass not fully implemented for outgoing MITM connections)
    let proxy = HudsuckerProxy::builder()
        .with_listener(listener)
        .with_ca(ca)
        .with_rustls_connector(crypto_provider)
        .with_http_handler(handler)
        .build()?;

    // Setup Ctrl+C handler
    let inventory_dir_clone = inventory_dir.clone();
    let handler_inventory_clone = handler_inventory.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        info!("Received Ctrl+C, saving inventory...");

        let inventory = handler_inventory_clone.lock().await;
        if let Err(e) = save_inventory(&*inventory, &inventory_dir_clone).await {
            error!("Failed to save inventory: {}", e);
        } else {
            info!("Inventory saved successfully");
        }

        std::process::exit(0);
    });

    // Start the proxy server
    info!("HTTPS MITM Proxy listening on 127.0.0.1:{}", actual_port);
    info!("Configure your client to trust the self-signed CA certificate");

    if let Err(e) = proxy.start().await {
        error!("Proxy server error: {}", e);
        return Err(e.into());
    }

    Ok(())
}

pub async fn save_inventory(inventory: &Inventory, inventory_dir: &PathBuf) -> Result<()> {
    let file_system = Arc::new(RealFileSystem);
    save_inventory_with_fs(inventory, inventory_dir, file_system).await
}

pub async fn save_inventory_with_fs<F: FileSystem>(
    inventory: &Inventory,
    inventory_dir: &PathBuf,
    file_system: Arc<F>,
) -> Result<()> {
    file_system.create_dir_all(inventory_dir).await?;

    let inventory_path = inventory_dir.join("inventory.json");
    // 2スペースインデントで整形
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    inventory.serialize(&mut ser)?;
    let inventory_json = String::from_utf8(buf)?;

    file_system.write_string(&inventory_path, &inventory_json).await?;

    Ok(())
}
