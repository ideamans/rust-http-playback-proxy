use anyhow::Result;
use tracing::{error, info};

use crate::ghost_server::GhostServerPool;
use crate::traits::FileSystem;
use crate::types::Transaction;

use super::ghost_forwarder::GhostForwarder;
use hudsucker::{
    Proxy as HudsuckerProxy,
    certificate_authority::RcgenAuthority,
    rcgen::{CertificateParams, DistinguishedName, Issuer, KeyPair},
    rustls::crypto::aws_lc_rs,
};

pub async fn start_playback_proxy<F: FileSystem + 'static>(
    port: u16,
    transactions: Vec<Transaction>,
    full_throttle: bool,
) -> Result<()> {
    info!("Starting playback mode with Ghost Server Pool architecture");

    // Phase 1: Start Ghost Server Pool (one server per domain)
    let ghost_pool = GhostServerPool::start(transactions, full_throttle).await?;
    let routing_table = ghost_pool.get_routing_table();

    info!(
        "Ghost Server Pool started with {} servers",
        routing_table.len()
    );

    // Phase 2: Start MITM proxy that forwards to Ghost Servers
    info!("Starting HTTPS MITM proxy on port {}", port);

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

    // Create the forwarding handler with routing table
    let handler = GhostForwarder::new(routing_table);

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
    info!("Configure your client to trust the self-signed CA certificate or use --insecure");

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
    info!("Shutdown signal received, stopping playback proxy");

    // Abort proxy task first
    proxy_task.abort();

    // Give in-flight requests a moment to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Stop Ghost Server Pool
    ghost_pool.stop().await;

    info!("Playback proxy stopped");

    Ok(())
}
