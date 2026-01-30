//! Ghost Server Request Handler
//!
//! Handles HTTP requests by matching them to recorded transactions
//! and serving responses with timing control.

use super::server::GhostServerState;
use bytes::Bytes;
use futures::stream;
use http_body_util::StreamBody;
use hyper::body::{Frame, Incoming};
use hyper::{Request, Response, StatusCode};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info};

/// Body type for responses
type ResponseBody = http_body_util::combinators::BoxBody<Bytes, std::io::Error>;

/// Handle an incoming HTTP request
pub async fn handle_request(
    req: Request<Incoming>,
    state: Arc<GhostServerState>,
) -> Result<Response<ResponseBody>, std::io::Error> {
    let method = req.method().to_string();
    let uri = req.uri().clone();
    let headers = req.headers();

    // Extract Host for virtual host matching
    // Prefer X-Original-Host (set by GhostForwarder) over Host header
    // because reqwest may override Host based on the request URL
    let host = headers
        .get("x-original-host")
        .or_else(|| headers.get("host"))
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    let path = uri.path();
    let query = uri.query();

    info!(
        "Ghost Server: {} {} (Host: {}, query: {:?})",
        method, path, host, query
    );

    // Find matching transaction
    let transaction = find_matching_transaction(&state.transactions, &method, host, path, query);

    match transaction {
        Some(transaction) => {
            info!("Found matching transaction for: {} {}", method, path);
            serve_transaction(transaction.clone(), state.full_throttle).await
        }
        None => {
            info!(
                "No transaction found for: {} {} (Host: {})",
                method, path, host
            );
            let body = format!(
                "Ghost Server: Resource not found\nMethod: {}\nHost: {}\nPath: {}",
                method, host, path
            );
            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("X-Ghost-Miss", "true")
                .body(box_body(body.into_bytes()))
                .unwrap())
        }
    }
}

/// Find a matching transaction for the request
fn find_matching_transaction<'a>(
    transactions: &'a [crate::types::Transaction],
    method: &str,
    request_host: &str,
    request_path: &str,
    request_query: Option<&str>,
) -> Option<&'a crate::types::Transaction> {
    transactions.iter().find(|t| {
        // Match method
        if t.method != method {
            return false;
        }

        // Parse transaction URL to extract components
        if let Ok(transaction_uri) = t.url.parse::<hyper::Uri>() {
            let t_path = transaction_uri.path();
            let t_query = transaction_uri.query();

            // Extract host from transaction URL
            let t_host = transaction_uri
                .authority()
                .map(|a| a.as_str())
                .unwrap_or("");

            // Remove port from hosts for comparison if present
            let request_host_no_port = request_host.split(':').next().unwrap_or(request_host);
            let t_host_no_port = t_host.split(':').next().unwrap_or(t_host);

            // Match host (case-insensitive)
            let host_matches = if request_host_no_port.is_empty() || t_host_no_port.is_empty() {
                // If either host is empty, fall back to path-only matching
                true
            } else {
                request_host_no_port.eq_ignore_ascii_case(t_host_no_port)
            };

            // Match path and query
            let path_matches = t_path == request_path;
            let query_matches = t_query == request_query;

            host_matches && path_matches && query_matches
        } else {
            false
        }
    })
}

/// Serve a transaction with timing control
async fn serve_transaction(
    transaction: crate::types::Transaction,
    full_throttle: bool,
) -> Result<Response<ResponseBody>, std::io::Error> {
    // Wait for TTFB before sending response headers
    let ttfb_ms = transaction.ttfb;
    if !full_throttle && ttfb_ms > 0 {
        info!("Waiting {}ms for TTFB", ttfb_ms);
        tokio::time::sleep(Duration::from_millis(ttfb_ms)).await;
    }

    // Record time after TTFB wait (chunks are relative to this point)
    let ttfb_end_instant = Instant::now();

    info!(
        "Serving transaction: status={:?}, chunks={}, target_close={}ms",
        transaction.status_code,
        transaction.chunks.len(),
        transaction.target_close_time
    );

    // If there's an error message, return error response
    if let Some(error_msg) = &transaction.error_message {
        error!("Transaction has error: {}", error_msg);
        return Ok(Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(box_body(error_msg.clone().into_bytes()))
            .unwrap());
    }

    // Build response with headers
    let mut response_builder = Response::builder().status(transaction.status_code.unwrap_or(200));

    // Add headers (skip hop-by-hop headers)
    if let Some(headers) = &transaction.raw_headers {
        for (key, value) in headers {
            let key_lower = key.to_lowercase();

            // Skip hop-by-hop headers
            if matches!(
                key_lower.as_str(),
                "transfer-encoding"
                    | "content-length"
                    | "connection"
                    | "keep-alive"
                    | "upgrade"
                    | "te"
                    | "trailer"
                    | "proxy-connection"
                    | "proxy-authorization"
                    | "proxy-authenticate"
            ) {
                continue;
            }

            // Add all values for this header
            if let Ok(header_name) = hyper::header::HeaderName::from_bytes(key.as_bytes()) {
                for val_str in value.as_vec() {
                    if let Ok(header_value) = hyper::header::HeaderValue::from_str(val_str) {
                        response_builder =
                            response_builder.header(header_name.clone(), header_value);
                    }
                }
            }
        }
    }

    // Create streaming body with timing control
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
            full_throttle,
        ),
        |(mut iter, ttfb_instant, close_time, total, chunk_idx, sent_all, full_throttle)| async move {
            if sent_all {
                // All chunks sent, wait until target_close_time before closing
                if !full_throttle {
                    let elapsed = ttfb_instant.elapsed().as_millis() as u64;
                    if close_time > elapsed {
                        let wait_time = close_time - elapsed;
                        info!(
                            "All {} chunks sent, waiting {}ms before close",
                            total, wait_time
                        );
                        tokio::time::sleep(Duration::from_millis(wait_time)).await;
                    }
                }
                return None;
            }

            if let Some(chunk) = iter.next() {
                // Wait until target_time for this chunk
                if !full_throttle {
                    let elapsed = ttfb_instant.elapsed().as_millis() as u64;
                    if chunk.target_time > elapsed {
                        let wait_time = chunk.target_time - elapsed;
                        tokio::time::sleep(Duration::from_millis(wait_time)).await;
                    }
                }

                let frame = Frame::data(Bytes::from(chunk.chunk));
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
                        full_throttle,
                    ),
                ))
            } else {
                None
            }
        },
    );

    let stream_body = StreamBody::new(stream);
    let boxed_body = http_body_util::BodyExt::boxed(stream_body);

    Ok(response_builder.body(boxed_body).unwrap())
}

/// Helper to create a boxed body from bytes
fn box_body(bytes: Vec<u8>) -> ResponseBody {
    use http_body_util::{BodyExt, Full};
    Full::new(Bytes::from(bytes))
        .map_err(|_: std::convert::Infallible| std::io::Error::other("infallible"))
        .boxed()
}
