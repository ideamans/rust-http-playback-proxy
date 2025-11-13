use bytes::Bytes;
use http_body_util::StreamBody;
use hudsucker::{
    Body, HttpContext, HttpHandler, RequestOrResponse,
    hyper::{Request, Response, StatusCode},
};
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::types::Transaction;
use futures::stream;
use hyper::body::Frame;

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
                    let matches = host_matches && t_path == request_path && t_query == request_query;
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
            Some(transaction) => {
                match serve_transaction(transaction, start_time).await {
                    Ok(response) => RequestOrResponse::Response(response),
                    Err(e) => {
                        error!("Error serving transaction: {}", e);
                        let response = Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::from(format!("Transaction error: {}", e)))
                            .unwrap();
                        RequestOrResponse::Response(response)
                    }
                }
            }
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

    fn handle_response(
        &mut self,
        _ctx: &HttpContext,
        res: Response<Body>,
    ) -> impl Future<Output = Response<Body>> + Send {
        async move {
            // Pass through responses unchanged
            res
        }
    }
}

async fn serve_transaction(
    transaction: Transaction,
    _start_time: Arc<Instant>,
) -> anyhow::Result<Response<Body>> {
    // Wait for TTFB before sending response headers
    // This ensures the client measures TTFB accurately
    let ttfb_ms = transaction.ttfb;
    info!(
        "Waiting {}ms for TTFB before sending response headers",
        ttfb_ms
    );
    tokio::time::sleep(Duration::from_millis(ttfb_ms)).await;
    info!("TTFB wait completed, now sending response headers");

    // Record the time after TTFB wait (when we start sending body)
    // Chunks have target_time relative to this point
    let ttfb_end_instant = Instant::now();

    info!("Serving transaction for URL: {}", transaction.url);
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

    // Create streaming body with timing control
    // Chunks have target_time as relative time from TTFB completion (0-based)
    // After all chunks are sent, wait until target_close_time before closing the connection
    let chunks = transaction.chunks.clone();
    let target_close_time = transaction.target_close_time;
    let total_chunks = chunks.len();

    let stream = stream::unfold(
        (
            chunks.into_iter().peekable(),
            ttfb_end_instant,
            target_close_time,
            total_chunks,
            0usize,
            false,
        ),
        |(mut iter, ttfb_instant, close_time, total, chunk_idx, sent_all)| async move {
            if sent_all {
                // All chunks have been sent, now wait until target_close_time before closing
                let elapsed = ttfb_instant.elapsed().as_millis() as u64;
                if close_time > elapsed {
                    let wait_time = close_time - elapsed;
                    info!(
                        "All {} chunks sent, waiting {}ms until target_close_time before closing connection",
                        total, wait_time
                    );
                    tokio::time::sleep(Duration::from_millis(wait_time)).await;
                } else {
                    let behind_ms = elapsed - close_time;
                    info!(
                        "All {} chunks sent, already {}ms past target_close_time, closing immediately",
                        total, behind_ms
                    );
                }
                // Stream ends here - connection will close
                return None;
            }

            if let Some(chunk) = iter.next() {
                // Check current elapsed time since TTFB completion
                let elapsed = ttfb_instant.elapsed().as_millis() as u64;

                // Wait until target_time for this chunk
                if chunk.target_time > elapsed {
                    let wait_time = chunk.target_time - elapsed;
                    info!(
                        "Chunk[{}]: Waiting {}ms before sending (target: {}ms, elapsed: {}ms)",
                        chunk_idx, wait_time, chunk.target_time, elapsed
                    );
                    tokio::time::sleep(Duration::from_millis(wait_time)).await;
                } else if chunk.target_time > 0 && elapsed > chunk.target_time {
                    // We're behind schedule - log it but send immediately
                    let behind_ms = elapsed - chunk.target_time;
                    info!(
                        "Chunk[{}]: Behind schedule by {}ms, sending immediately (target: {}ms, elapsed: {}ms)",
                        chunk_idx, behind_ms, chunk.target_time, elapsed
                    );
                }

                // Send chunk
                info!("Chunk[{}]: Sending {} bytes", chunk_idx, chunk.chunk.len());
                let frame = Frame::data(Bytes::from(chunk.chunk));

                // Check if this was the last chunk
                let is_last = iter.peek().is_none();

                Some((
                    Ok::<_, std::io::Error>(frame),
                    (
                        iter,
                        ttfb_instant,
                        close_time,
                        total,
                        chunk_idx + 1,
                        is_last,
                    ),
                ))
            } else {
                // Shouldn't reach here but handle gracefully
                None
            }
        },
    );

    let stream_body = StreamBody::new(stream);

    // Convert to Hudsucker's Body type using from_stream
    // Map the stream to extract bytes from frames
    use futures::TryStreamExt;
    let bytes_stream = stream_body.map_ok(|frame| {
        frame.into_data().unwrap_or_default()
    });

    let body = Body::from_stream(bytes_stream);

    let response = response_builder.body(body)?;

    Ok(response)
}
