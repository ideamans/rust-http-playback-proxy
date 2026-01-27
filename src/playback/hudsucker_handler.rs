use bytes::Bytes;
use hudsucker::{
    Body, HttpContext, HttpHandler, RequestOrResponse,
    hyper::{Request, Response, StatusCode},
};
use std::future::Future;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::playback::timing_delivery::{TimedChunk, spawn_timed_delivery_with_ttfb};
use crate::types::Transaction;

/// Playback handler for Hudsucker MITM proxy
#[derive(Clone)]
pub struct PlaybackHandler {
    transactions: Arc<RwLock<Arc<Vec<Transaction>>>>,
    start_time: Arc<Instant>,
}

impl PlaybackHandler {
    pub fn new(transactions: Vec<Transaction>) -> Self {
        Self {
            transactions: Arc::new(RwLock::new(Arc::new(transactions))),
            start_time: Arc::new(Instant::now()),
        }
    }
}

impl HttpHandler for PlaybackHandler {
    fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        req: Request<Body>,
    ) -> impl Future<Output = RequestOrResponse> + Send {
        let transactions = self.transactions.clone();
        let start_time = self.start_time.clone();

        async move {
            let method = req.method().to_string();
            let uri = req.uri().clone();
            let headers = req.headers();

            // Skip CONNECT requests - they are for tunnel establishment, not actual HTTP requests
            if method == "CONNECT" {
                info!("Skipping CONNECT request (tunnel): {}", uri);
                return RequestOrResponse::Request(req);
            }

            // Reconstruct full URL from URI and Host header (including query parameters)
            let url = if uri.scheme().is_some() {
                // Full URL in request (proxy-style)
                uri.to_string()
            } else {
                // Relative URL - reconstruct from Host header
                if let Some(host) = headers.get("host") {
                    if let Ok(host_str) = host.to_str() {
                        // Use https by default for recorded resources
                        // Include query parameters if present
                        if let Some(query) = uri.query() {
                            format!("https://{}{}?{}", host_str, uri.path(), query)
                        } else {
                            format!("https://{}{}", host_str, uri.path())
                        }
                    } else {
                        uri.to_string()
                    }
                } else {
                    uri.to_string()
                }
            };

            info!(
                "Handling playback request: {} {} (reconstructed URL: {})",
                method, uri, url
            );

            // Extract request components for matching
            let request_path = uri.path();
            let request_query = uri.query();
            let request_host = headers
                .get("host")
                .and_then(|h| h.to_str().ok())
                .or_else(|| uri.authority().map(|a| a.as_str()));

            info!(
                "Looking for transaction: method={}, host={:?}, path={}, query={:?}",
                method, request_host, request_path, request_query
            );

            // Read transactions with RwLock
            let transactions_snapshot = {
                let txn_read = transactions.read().await;
                txn_read.clone() // Clone the Arc<Vec<Transaction>>
            };

            info!(
                "Total transactions available: {}",
                transactions_snapshot.len()
            );

            // Debug: List all available transactions
            for (idx, t) in transactions_snapshot.iter().enumerate() {
                if let Ok(transaction_uri) = t.url.parse::<hyper::Uri>() {
                    let t_host = transaction_uri.authority().map(|a| a.as_str());
                    info!(
                        "  Transaction[{}]: method={}, host={:?}, url={}, path={}, query={:?}",
                        idx,
                        t.method,
                        t_host,
                        t.url,
                        transaction_uri.path(),
                        transaction_uri.query()
                    );
                }
            }

            let transaction = transactions_snapshot
                .iter()
                .find(|t| {
                    // Match method
                    if t.method != method {
                        return false;
                    }

                    // Parse transaction URL to extract components
                    if let Ok(transaction_uri) = t.url.parse::<hyper::Uri>() {
                        let t_path = transaction_uri.path();
                        let t_query = transaction_uri.query();
                        let t_host = transaction_uri.authority().map(|a| a.as_str());

                        // Match host (if available in both request and transaction)
                        // This prevents cross-origin mismatches
                        let host_matches = match (request_host, t_host) {
                            (Some(req_h), Some(t_h)) => req_h == t_h,
                            // If either is missing, fall back to path-only matching for backward compatibility
                            _ => true,
                        };

                        // Match path and query
                        let matches =
                            host_matches && t_path == request_path && t_query == request_query;
                        if matches {
                            info!("Found matching transaction: {}", t.url);
                        }
                        matches
                    } else {
                        false
                    }
                })
                .cloned();

            match transaction {
                Some(transaction) => match serve_transaction(transaction, start_time).await {
                    Ok(response) => RequestOrResponse::Response(response),
                    Err(e) => {
                        error!("Error serving transaction: {}", e);
                        let response = Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::from(format!("Transaction error: {}", e)))
                            .unwrap();
                        RequestOrResponse::Response(response)
                    }
                },
                None => {
                    info!(
                        "No transaction found for: {} {} (url: {})",
                        method, uri, url
                    );
                    let response = Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::from(format!(
                            "Resource not found in playback data: {} {}",
                            method, url
                        )))
                        .unwrap();
                    RequestOrResponse::Response(response)
                }
            }
        }
    }

    async fn handle_response(&mut self, _ctx: &HttpContext, res: Response<Body>) -> Response<Body> {
        // Pass through responses unchanged
        res
    }
}

async fn serve_transaction(
    transaction: Transaction,
    _start_time: Arc<Instant>,
) -> anyhow::Result<Response<Body>> {
    // NOTE: TTFB waiting is now handled in the spawned delivery task, not here.
    // This allows handle_request to return immediately, preventing HTTP/2 connection blocking.
    let ttfb_ms = transaction.ttfb;

    info!("Serving transaction for URL: {}", transaction.url);
    info!("  TTFB: {}ms (will be simulated in body stream)", ttfb_ms);
    info!("  Status code: {:?}", transaction.status_code);
    info!("  Number of chunks: {}", transaction.chunks.len());
    info!(
        "  Target close time: {}ms (relative to TTFB)",
        transaction.target_close_time
    );

    // If there's an error message, return error response
    if let Some(error_msg) = &transaction.error_message {
        error!("Transaction has error message: {}", error_msg);
        return Ok(Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(error_msg.clone()))?);
    }

    // Build response
    let mut response_builder = Response::builder().status(transaction.status_code.unwrap_or(200));

    // Add headers (skip hop-by-hop headers that Hyper manages automatically)
    if let Some(headers) = &transaction.raw_headers {
        for (key, value) in headers {
            // Skip headers that Hyper manages automatically to avoid UnexpectedHeader error
            let key_lower = key.to_lowercase();
            // Extended list of hop-by-hop headers per RFC 2616 Section 13.5.1
            // and additional headers that Hyper manages
            if key_lower == "transfer-encoding"
                || key_lower == "content-length"
                || key_lower == "connection"
                || key_lower == "keep-alive"
                || key_lower == "upgrade"
                || key_lower == "te"
                || key_lower == "trailer"
                || key_lower == "proxy-connection"
                || key_lower == "proxy-authorization"
                || key_lower == "proxy-authenticate"
                || key_lower == "host"
            // Host header can cause issues in responses
            {
                continue; // Skip hop-by-hop headers
            }

            // Validate header name and add all values (handles both Single and Multiple)
            if let Ok(header_name) = hyper::header::HeaderName::from_bytes(key.as_bytes()) {
                // Add all values for this header (supports multiple values like Set-Cookie)
                for val_str in value.as_vec() {
                    if let Ok(header_value) = hyper::header::HeaderValue::from_str(val_str) {
                        response_builder =
                            response_builder.header(header_name.clone(), header_value);
                    }
                }
            }
        }
    }

    // Log chunk details
    for (idx, chunk) in transaction.chunks.iter().enumerate() {
        info!(
            "  Chunk[{}]: size={} bytes, target_time={}ms (relative to TTFB)",
            idx,
            chunk.chunk.len(),
            chunk.target_time
        );
    }

    // Convert chunks to TimedChunks for the channel-based delivery
    let timed_chunks: Vec<TimedChunk> = transaction
        .chunks
        .into_iter()
        .map(|c| TimedChunk {
            data: Bytes::from(c.chunk),
            target_time: c.target_time,
        })
        .collect();

    let target_close_time = transaction.target_close_time;

    // Spawn a background task that handles TTFB waiting AND chunk delivery.
    // This solves HTTP/2 blocking: ALL timing waits happen in a spawned task,
    // not in handle_request, so other HTTP/2 streams aren't blocked.
    // Note: Response headers are sent immediately, TTFB is simulated by delaying first byte.
    let stream = spawn_timed_delivery_with_ttfb(ttfb_ms, timed_chunks, target_close_time);

    // Body::from_stream expects Stream<Item = Result<impl Into<Bytes>, impl Error>>
    // Our stream already yields Result<Bytes, std::io::Error> which satisfies this
    let body = Body::from_stream(stream);

    let response = response_builder.body(body)?;

    Ok(response)
}
