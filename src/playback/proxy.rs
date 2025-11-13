use anyhow::Result;
use tracing::{error, info};

use crate::traits::FileSystem;
use crate::types::Transaction;

use super::hudsucker_handler::PlaybackHandler;
use hudsucker::{
    Proxy as HudsuckerProxy,
    certificate_authority::RcgenAuthority,
    rcgen::{CertificateParams, DistinguishedName, Issuer, KeyPair},
    rustls::crypto::aws_lc_rs,
};

pub async fn start_playback_proxy<F: FileSystem + 'static>(
    port: u16,
    transactions: Vec<Transaction>,
) -> Result<()> {
    info!("Starting HTTPS MITM playback proxy on port {}", port);

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

    // Create the playback handler
    let handler = PlaybackHandler::new(transactions);

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

    // Note: Hudsucker proxy doesn't provide graceful shutdown mechanism
    // We rely on the process termination to stop accepting connections
    // Give in-flight requests a moment to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    info!("Playback proxy stopped");

    // Abort proxy task
    proxy_task.abort();

    Ok(())
}
