//! Ghost Server Pool - Multiple HTTPS servers per domain
//!
//! Manages multiple Ghost HTTPS servers, one per unique (scheme, host) combination.
//! This provides realistic TLS handshake simulation for each domain.

use crate::types::{HttpVersion, Transaction};
use anyhow::Result;
use hudsucker::rcgen::{CertificateParams, DistinguishedName, KeyPair};
use hudsucker::rustls::crypto::aws_lc_rs;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tracing::{error, info, warn};

use super::handler::handle_request;
use super::server::GhostServerState;

/// Origin key: (scheme, host) - e.g., ("https", "example.com")
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct OriginKey {
    pub scheme: String,
    pub host: String,
}

impl OriginKey {
    pub fn from_url(url: &str) -> Option<Self> {
        let parsed = url::Url::parse(url).ok()?;
        let scheme = parsed.scheme().to_string();
        let host = parsed.host_str()?.to_string();
        Some(Self { scheme, host })
    }
}

/// Information about a running Ghost Server for a specific origin
#[derive(Debug)]
pub struct OriginServer {
    pub origin: OriginKey,
    pub port: u16,
    pub http_version: HttpVersion,
    pub shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    pub server_handle: Option<tokio::task::JoinHandle<()>>,
}

/// Pool of Ghost HTTPS servers, one per origin
pub struct GhostServerPool {
    /// Map from origin to server info
    servers: HashMap<OriginKey, OriginServer>,
    /// Map from origin to port (for quick lookup by GhostForwarder)
    origin_to_port: HashMap<OriginKey, u16>,
}

impl GhostServerPool {
    /// Start Ghost Servers for all unique origins in the transactions
    pub async fn start(transactions: Vec<Transaction>) -> Result<Self> {
        // Install the CryptoProvider for rustls
        let _ = aws_lc_rs::default_provider().install_default();

        let start_time = Instant::now();

        // Group transactions by origin and determine HTTP version
        let mut origin_transactions: HashMap<OriginKey, (Vec<Transaction>, HttpVersion)> =
            HashMap::new();

        for tx in transactions {
            if let Some(origin) = OriginKey::from_url(&tx.url) {
                let entry = origin_transactions
                    .entry(origin.clone())
                    .or_insert_with(|| (Vec::new(), HttpVersion::Http11));

                // Use HTTP/2 if any resource from this origin used HTTP/2
                // (conservative: if mixed, prefer HTTP/2)
                if let Some(version) = get_http_version_from_transaction(&tx) {
                    if version == HttpVersion::Http2 {
                        entry.1 = HttpVersion::Http2;
                    }
                }

                entry.0.push(tx);
            }
        }

        let origin_count = origin_transactions.len();
        info!(
            "Starting {} Ghost Servers for unique origins...",
            origin_count
        );

        // Start servers in parallel
        let mut server_futures = Vec::new();

        for (origin, (txs, http_version)) in origin_transactions {
            let origin_clone = origin.clone();
            server_futures.push(tokio::spawn(async move {
                start_origin_server(origin_clone, txs, http_version).await
            }));
        }

        // Wait for all servers to start
        let mut servers = HashMap::new();
        let mut origin_to_port = HashMap::new();

        for future in server_futures {
            match future.await {
                Ok(Ok(server)) => {
                    let origin = server.origin.clone();
                    let port = server.port;
                    origin_to_port.insert(origin.clone(), port);
                    servers.insert(origin, server);
                }
                Ok(Err(e)) => {
                    error!("Failed to start origin server: {}", e);
                }
                Err(e) => {
                    error!("Server start task panicked: {}", e);
                }
            }
        }

        let elapsed = start_time.elapsed();
        info!(
            "Started {} Ghost Servers in {:.2}ms",
            servers.len(),
            elapsed.as_secs_f64() * 1000.0
        );

        // Log the mapping
        for (origin, port) in &origin_to_port {
            info!("  {}://{} -> port {}", origin.scheme, origin.host, port);
        }

        Ok(Self {
            servers,
            origin_to_port,
        })
    }

    /// Get the port for a given origin
    pub fn get_port(&self, origin: &OriginKey) -> Option<u16> {
        self.origin_to_port.get(origin).copied()
    }

    /// Get the port for a given host (tries both https and http)
    pub fn get_port_for_host(&self, host: &str) -> Option<u16> {
        // Try HTTPS first (most common)
        let https_origin = OriginKey {
            scheme: "https".to_string(),
            host: host.to_string(),
        };
        if let Some(port) = self.origin_to_port.get(&https_origin) {
            return Some(*port);
        }

        // Try HTTP
        let http_origin = OriginKey {
            scheme: "http".to_string(),
            host: host.to_string(),
        };
        self.origin_to_port.get(&http_origin).copied()
    }

    /// Get the origin-to-port mapping (for GhostForwarder)
    pub fn get_routing_table(&self) -> HashMap<String, u16> {
        self.origin_to_port
            .iter()
            .map(|(origin, port)| (origin.host.clone(), *port))
            .collect()
    }

    /// Stop all servers
    pub async fn stop(mut self) {
        let start_time = Instant::now();
        let server_count = self.servers.len();

        info!("Stopping {} Ghost Servers...", server_count);

        // Send shutdown signals to all servers
        for server in self.servers.values_mut() {
            if let Some(tx) = server.shutdown_tx.take() {
                let _ = tx.send(());
            }
        }

        // Wait for all server tasks to complete (with timeout)
        let mut handles = Vec::new();
        for (_, mut server) in self.servers.drain() {
            if let Some(handle) = server.server_handle.take() {
                handles.push(handle);
            }
        }

        // Give servers time to shut down
        let shutdown_timeout = Duration::from_secs(2);
        let _ = tokio::time::timeout(shutdown_timeout, futures::future::join_all(handles)).await;

        let elapsed = start_time.elapsed();
        info!(
            "Stopped {} Ghost Servers in {:.2}ms",
            server_count,
            elapsed.as_secs_f64() * 1000.0
        );
    }
}

/// Start a Ghost HTTPS Server for a specific origin
async fn start_origin_server(
    origin: OriginKey,
    transactions: Vec<Transaction>,
    http_version: HttpVersion,
) -> Result<OriginServer> {
    // Find an available port
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let port = addr.port();

    info!(
        "Starting Ghost Server for {}://{} on port {} ({:?})",
        origin.scheme, origin.host, port, http_version
    );

    // Generate TLS certificate for this domain
    let tls_acceptor = create_tls_acceptor(&origin.host)?;

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Create shared state
    let state = Arc::new(GhostServerState {
        transactions: Arc::new(transactions),
        start_time: Instant::now(),
    });

    // Spawn server task
    let server_handle = tokio::spawn(run_https_server(
        listener,
        tls_acceptor,
        state,
        shutdown_rx,
        http_version,
    ));

    Ok(OriginServer {
        origin,
        port,
        http_version,
        shutdown_tx: Some(shutdown_tx),
        server_handle: Some(server_handle),
    })
}

/// Create a TLS acceptor with a self-signed certificate for the given domain
fn create_tls_acceptor(domain: &str) -> Result<TlsAcceptor> {
    // Generate key pair
    let key_pair = KeyPair::generate()?;

    // Create certificate parameters
    let mut params = CertificateParams::new(vec![domain.to_string()])?;
    let mut dn = DistinguishedName::new();
    dn.push(hudsucker::rcgen::DnType::CommonName, domain);
    params.distinguished_name = dn;

    // Generate self-signed certificate
    let cert = params.self_signed(&key_pair)?;

    // Convert to rustls types
    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::try_from(key_pair.serialize_der())
        .map_err(|e| anyhow::anyhow!("Failed to convert key: {:?}", e))?;

    // Create rustls config
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

/// Run the HTTPS server loop
async fn run_https_server(
    listener: TcpListener,
    tls_acceptor: TlsAcceptor,
    state: Arc<GhostServerState>,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    _http_version: HttpVersion,
) {
    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, remote_addr)) => {
                        let acceptor = tls_acceptor.clone();
                        let state = state.clone();

                        tokio::spawn(async move {
                            // Perform TLS handshake
                            match acceptor.accept(stream).await {
                                Ok(tls_stream) => {
                                    handle_tls_connection(tls_stream, remote_addr, state).await;
                                }
                                Err(e) => {
                                    // TLS handshake errors are common (client disconnects, etc.)
                                    warn!("TLS handshake error from {}: {}", remote_addr, e);
                                }
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept connection: {}", e);
                    }
                }
            }

            _ = &mut shutdown_rx => {
                break;
            }
        }
    }
}

/// Handle a single TLS connection
async fn handle_tls_connection(
    stream: tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
    remote_addr: SocketAddr,
    state: Arc<GhostServerState>,
) {
    let io = TokioIo::new(stream);

    let service = service_fn(move |req| {
        let state = state.clone();
        async move { handle_request(req, state).await }
    });

    // For now, always use HTTP/1.1
    // TODO: Add HTTP/2 support with hyper::server::conn::http2
    if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
        if !err.is_incomplete_message() {
            error!(
                "Error serving TLS connection from {}: {:?}",
                remote_addr, err
            );
        }
    }
}

/// Extract HTTP version from a transaction (via URL-based heuristic or stored metadata)
fn get_http_version_from_transaction(_tx: &Transaction) -> Option<HttpVersion> {
    // TODO: Store HTTP version in Transaction and read it here
    // For now, return None and let the caller use default
    None
}
