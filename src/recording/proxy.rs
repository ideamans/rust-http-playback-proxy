use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info};

use super::hudsucker_handler::RecordingHandler;
use crate::traits::{FileSystem, RealFileSystem};
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
    control_port: Option<u16>,
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
    let handler = RecordingHandler::new(inventory, inventory_dir.clone());
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

    // Create shutdown channel
    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let mut shutdown_rx = shutdown_tx.subscribe();

    // Start management API server for shutdown (only if control_port is specified)
    if let Some(mgmt_port) = control_port {
        let mgmt_shutdown_tx = shutdown_tx.clone();
        tokio::spawn(async move {
            use http_body_util::Full;
            use hyper::body::Bytes;
            use hyper::body::Incoming;
            use hyper::server::conn::http1;
            use hyper::service::service_fn;
            use hyper::{Request, Response};
            use hyper_util::rt::TokioIo;
            use std::convert::Infallible;

            async fn handle_request(
                req: Request<Incoming>,
                shutdown_tx: broadcast::Sender<()>,
            ) -> Result<Response<Full<Bytes>>, Infallible> {
                if req.uri().path() == "/_shutdown" && req.method() == hyper::Method::POST {
                    info!("Received shutdown request via management API");
                    let _ = shutdown_tx.send(());
                    Ok(Response::new(Full::new(Bytes::from("Shutting down...\n"))))
                } else {
                    Ok(Response::builder()
                        .status(404)
                        .body(Full::new(Bytes::from("Not found\n")))
                        .unwrap())
                }
            }

            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], mgmt_port));
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    error!("Failed to bind management API: {}", e);
                    return;
                }
            };
            info!(
                "Management API listening on http://127.0.0.1:{} (POST /_shutdown to stop)",
                mgmt_port
            );

            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        error!("Failed to accept connection: {}", e);
                        continue;
                    }
                };

                let io = TokioIo::new(stream);
                let shutdown_tx = mgmt_shutdown_tx.clone();

                tokio::spawn(async move {
                    let service = service_fn(move |req| handle_request(req, shutdown_tx.clone()));

                    if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                        error!("Error serving connection: {}", e);
                    }
                });
            }
        });
    }

    // Setup Ctrl+C handler and shutdown listener
    let inventory_dir_clone = inventory_dir.clone();
    let handler_inventory_clone = handler_inventory.clone();
    let shutdown_tx_clone = shutdown_tx.clone();

    let shutdown_handler = tokio::spawn(async move {
        tokio::select! {
            _ = super::signal_handler::wait_for_shutdown_signal() => {
                info!("Received Ctrl+C signal");
            }
            _ = shutdown_rx.recv() => {
                info!("Received shutdown request");
            }
        }

        // Trigger proxy shutdown by sending to all subscribers
        let _ = shutdown_tx_clone.send(());

        // Wait a bit for in-flight requests to complete
        // This ensures all resources are recorded before saving inventory
        info!("Waiting for in-flight requests to complete...");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        info!("Saving inventory...");

        let inventory = handler_inventory_clone.lock().await;
        if let Err(e) = save_inventory(&inventory, &inventory_dir_clone).await {
            error!("Failed to save inventory: {}", e);
        } else {
            info!(
                "Inventory saved successfully with {} resources",
                inventory.resources.len()
            );
        }

        // Wait for async file writes to complete before exiting
        // Check for content files every second, up to 10 times
        let mut all_files_exist = false;

        for attempt in 1..=10 {
            let mut missing_count = 0;

            // Check if all resources have their content files saved
            for resource in &inventory.resources {
                if let Some(content_path) = &resource.content_file_path {
                    let full_path = inventory_dir_clone.join(content_path);
                    if !tokio::fs::try_exists(&full_path).await.unwrap_or(false) {
                        missing_count += 1;
                    }
                }
            }

            if missing_count == 0 {
                info!("All content files verified (attempt {})", attempt);
                all_files_exist = true;
                break;
            } else {
                info!(
                    "Waiting for {} content files to be written (attempt {}/10)",
                    missing_count, attempt
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        }

        if !all_files_exist {
            error!("Some content files may not have been written after 10 seconds");
        }
    });

    // Start the proxy server
    info!("HTTPS MITM Proxy listening on 127.0.0.1:{}", actual_port);
    info!("Configure your client to trust the self-signed CA certificate");

    // Run proxy with graceful shutdown support
    let mut proxy_shutdown_rx = shutdown_tx.subscribe();

    tokio::select! {
        result = proxy.start() => {
            if let Err(e) = result {
                error!("Proxy server error: {}", e);
                return Err(e.into());
            }
        }
        _ = proxy_shutdown_rx.recv() => {
            info!("Proxy received shutdown signal");
            // Proxy will stop accepting new connections
            // The shutdown handler above will save the inventory
        }
    }

    // Wait for shutdown handler to complete (inventory save, file verification, etc.)
    info!("Waiting for shutdown handler to complete...");
    if let Err(e) = shutdown_handler.await {
        error!("Shutdown handler error: {}", e);
    }
    info!("Shutdown complete");

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
