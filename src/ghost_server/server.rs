//! Ghost Server - HTTP Server Implementation
//!
//! A virtual host HTTP server that serves recorded resources with timing control.

use crate::types::Transaction;
use anyhow::Result;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tracing::{error, info};

use super::handler::handle_request;

/// Ghost Server - Virtual Host HTTP Server
pub struct GhostServer {
    /// Server address
    addr: SocketAddr,
    /// Shutdown sender
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Server task handle
    server_handle: Option<tokio::task::JoinHandle<()>>,
}

/// Shared state for the Ghost Server
pub struct GhostServerState {
    /// All transactions loaded from inventory
    pub transactions: Arc<Vec<Transaction>>,
    /// Server start time for relative timing
    pub start_time: Instant,
    /// When true, skip all timing delays (TTFB, chunk timing, close timing)
    pub full_throttle: bool,
}

impl GhostServer {
    /// Start the Ghost Server on the specified port
    pub async fn start(
        port: u16,
        transactions: Vec<Transaction>,
        full_throttle: bool,
    ) -> Result<Self> {
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
        let listener = TcpListener::bind(addr).await?;
        let actual_addr = listener.local_addr()?;

        info!("Ghost Server starting on http://{}", actual_addr);

        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        // Create shared state
        let state = Arc::new(GhostServerState {
            transactions: Arc::new(transactions),
            start_time: Instant::now(),
            full_throttle,
        });

        // Spawn server task
        let server_handle = tokio::spawn(run_server(listener, state, shutdown_rx));

        Ok(Self {
            addr: actual_addr,
            shutdown_tx: Some(shutdown_tx),
            server_handle: Some(server_handle),
        })
    }

    /// Get the server's bound address
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Get the server's port
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Stop the server gracefully
    pub async fn stop(mut self) -> Result<()> {
        info!("Stopping Ghost Server...");

        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        // Wait for server task to complete
        if let Some(handle) = self.server_handle.take() {
            // Give it a moment to clean up
            tokio::select! {
                _ = handle => {
                    info!("Ghost Server stopped gracefully");
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                    info!("Ghost Server shutdown timed out");
                }
            }
        }

        Ok(())
    }
}

impl Drop for GhostServer {
    fn drop(&mut self) {
        // Send shutdown signal if not already sent
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Run the HTTP server loop
async fn run_server(
    listener: TcpListener,
    state: Arc<GhostServerState>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    info!("Ghost Server accepting connections");

    loop {
        tokio::select! {
            // Accept new connection
            result = listener.accept() => {
                match result {
                    Ok((stream, remote_addr)) => {
                        let state = state.clone();
                        tokio::spawn(async move {
                            handle_connection(stream, remote_addr, state).await;
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept connection: {}", e);
                    }
                }
            }

            // Shutdown signal
            _ = &mut shutdown_rx => {
                info!("Ghost Server received shutdown signal");
                break;
            }
        }
    }

    info!("Ghost Server loop ended");
}

/// Handle a single connection
async fn handle_connection(
    stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    state: Arc<GhostServerState>,
) {
    let io = TokioIo::new(stream);

    // Create service function that captures state
    let service = service_fn(move |req| {
        let state = state.clone();
        async move { handle_request(req, state).await }
    });

    // Serve the connection
    if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
        // Connection errors are common (client disconnects, etc.)
        // Only log at debug level unless it's unexpected
        if !err.is_incomplete_message() {
            error!("Error serving connection from {}: {:?}", remote_addr, err);
        }
    }
}

#[cfg(test)]
mod server_tests {
    use super::*;

    #[tokio::test]
    async fn test_ghost_server_start_stop() {
        // Create empty transactions
        let transactions = vec![];

        // Start server on random port
        let server = GhostServer::start(0, transactions, false)
            .await
            .expect("Failed to start server");

        // Verify server is running
        let port = server.port();
        assert!(port > 0);

        // Stop server
        server.stop().await.expect("Failed to stop server");
    }
}
