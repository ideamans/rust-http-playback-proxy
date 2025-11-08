use anyhow::Result;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper::body::Incoming;
use http_body_util::StreamBody;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use std::sync::Arc;
use std::time::{Instant, Duration};
use tracing::{info, error};
use hyper::body::Frame;
use futures::stream;

use crate::types::Transaction;

pub async fn start_playback_proxy(port: u16, transactions: Vec<Transaction>) -> Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    let actual_addr = listener.local_addr()?;
    let actual_port = actual_addr.port();

    info!("Playback proxy listening on 127.0.0.1:{}", actual_port);
    
    let shared_transactions = Arc::new(transactions);
    let start_time = Arc::new(Instant::now());

    loop {
        let (stream, _) = listener.accept().await?;
        let shared_transactions = shared_transactions.clone();
        let start_time = start_time.clone();

        tokio::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(
                    TokioIo::new(stream),
                    service_fn(|req| {
                        handle_playback_request(req, shared_transactions.clone(), start_time.clone())
                    }),
                )
                .await
            {
                error!("Error serving connection: {:?}", err);
            }
        });
    }
}

async fn handle_playback_request(
    req: Request<Incoming>,
    transactions: Arc<Vec<Transaction>>,
    start_time: Arc<Instant>,
) -> Result<Response<http_body_util::combinators::BoxBody<bytes::Bytes, std::io::Error>>, hyper::Error> {
    use http_body_util::BodyExt;

    let method = req.method().to_string();
    let uri = req.uri().clone();
    let headers = req.headers();

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

    info!("Handling playback request: {} {} (reconstructed URL: {})", method, uri, url);

    // Find matching transaction by method and path (ignore protocol/host/port differences)
    // This allows matching regardless of HTTP vs HTTPS or different ports
    let request_path = uri.path();
    let request_query = uri.query();

    info!("Looking for transaction: method={}, path={}, query={:?}", method, request_path, request_query);
    info!("Total transactions available: {}", transactions.len());

    // Debug: List all available transactions
    for (idx, t) in transactions.iter().enumerate() {
        if let Ok(transaction_uri) = t.url.parse::<hyper::Uri>() {
            info!("  Transaction[{}]: method={}, url={}, path={}, query={:?}",
                idx, t.method, t.url, transaction_uri.path(), transaction_uri.query());
        }
    }

    let transaction = transactions
        .iter()
        .find(|t| {
            // Match method
            if t.method != method {
                return false;
            }

            // Parse transaction URL to extract path and query
            if let Ok(transaction_uri) = t.url.parse::<hyper::Uri>() {
                let t_path = transaction_uri.path();
                let t_query = transaction_uri.query();

                // Match path and query
                let matches = t_path == request_path && t_query == request_query;
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
                Ok(response) => Ok(response),
                Err(e) => {
                    error!("Error serving transaction: {}", e);
                    Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(
                            http_body_util::Full::new(bytes::Bytes::from(format!("Transaction error: {}", e)))
                                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                                .boxed()
                        )
                        .unwrap())
                }
            }
        }
        None => {
            info!("No transaction found for: {} {} (url: {})", method, uri, url);
            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(
                    http_body_util::Full::new(bytes::Bytes::from(format!("Resource not found in playback data: {} {}", method, url)))
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                        .boxed()
                )
                .unwrap())
        }
    }
}

async fn serve_transaction(
    transaction: Transaction,
    _start_time: Arc<Instant>,
) -> Result<Response<http_body_util::combinators::BoxBody<bytes::Bytes, std::io::Error>>> {
    use http_body_util::BodyExt;

    // Wait for TTFB before sending response headers
    // This ensures the client measures TTFB accurately
    let ttfb_ms = transaction.ttfb;
    info!("Waiting {}ms for TTFB before sending response headers", ttfb_ms);
    tokio::time::sleep(Duration::from_millis(ttfb_ms)).await;
    info!("TTFB wait completed, now sending response headers");

    // Record the time after TTFB wait (when we start sending body)
    // Chunks have target_time relative to this point
    let ttfb_end_instant = Instant::now();

    info!("Serving transaction for URL: {}", transaction.url);
    info!("  Status code: {:?}", transaction.status_code);
    info!("  Number of chunks: {}", transaction.chunks.len());
    info!("  Target close time: {}ms (relative to TTFB)", transaction.target_close_time);

    // If there's an error message, return error response
    if let Some(error_msg) = &transaction.error_message {
        error!("Transaction has error message: {}", error_msg);
        return Ok(Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(
                http_body_util::Full::new(bytes::Bytes::from(error_msg.clone()))
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                    .boxed()
            )?);
    }

    // Build response
    let mut response_builder = Response::builder()
        .status(transaction.status_code.unwrap_or(200));

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
                || key_lower == "host"  // Host header can cause issues in responses
            {
                continue; // Skip hop-by-hop headers
            }

            // Validate header name and value before adding
            if let Ok(header_name) = hyper::header::HeaderName::from_bytes(key.as_bytes()) {
                if let Ok(header_value) = hyper::header::HeaderValue::from_str(value) {
                    response_builder = response_builder.header(header_name, header_value);
                }
            }
        }
    }

    // Log chunk details
    for (idx, chunk) in transaction.chunks.iter().enumerate() {
        info!("  Chunk[{}]: size={} bytes, target_time={}ms (relative to TTFB)", idx, chunk.chunk.len(), chunk.target_time);
    }

    // Create streaming body with timing control
    // Chunks have target_time as relative time from TTFB completion (0-based)
    let chunks = transaction.chunks.clone();
    let target_close_time = transaction.target_close_time;

    let stream = stream::unfold(
        (chunks.into_iter().peekable(), ttfb_end_instant, target_close_time, false, 0usize),
        |(mut iter, ttfb_instant, close_time, is_done, chunk_idx)| async move {
            if is_done {
                info!("Stream finished, all chunks sent");
                return None;
            }

            if let Some(chunk) = iter.next() {
                // Check if this is the last chunk by peeking ahead
                let is_last_chunk = iter.peek().is_none();

                // Check current elapsed time since TTFB completion
                let elapsed = ttfb_instant.elapsed().as_millis() as u64;

                if is_last_chunk {
                    // For the last chunk, wait until target_close_time before sending it
                    // This ensures the client measures the full transfer duration
                    if close_time > elapsed {
                        let wait_time = close_time - elapsed;
                        info!("Chunk[{}] (LAST): Waiting {}ms until target_close_time before sending", chunk_idx, wait_time);
                        tokio::time::sleep(Duration::from_millis(wait_time)).await;
                    } else {
                        let behind_ms = elapsed - close_time;
                        info!("Chunk[{}] (LAST): Behind target_close_time by {}ms, sending immediately", chunk_idx, behind_ms);
                    }
                } else {
                    // For non-last chunks, use normal timing
                    if chunk.target_time > elapsed {
                        let wait_time = chunk.target_time - elapsed;
                        info!("Chunk[{}]: Waiting {}ms before sending (target: {}ms, elapsed: {}ms)",
                               chunk_idx, wait_time, chunk.target_time, elapsed);
                        tokio::time::sleep(Duration::from_millis(wait_time)).await;
                    } else if chunk.target_time > 0 && elapsed > chunk.target_time {
                        // We're behind schedule - log it but send immediately
                        let behind_ms = elapsed - chunk.target_time;
                        info!("Chunk[{}]: Behind schedule by {}ms, sending immediately (target: {}ms, elapsed: {}ms)",
                               chunk_idx, behind_ms, chunk.target_time, elapsed);
                    }
                }

                // Send chunk
                info!("Chunk[{}]: Sending {} bytes", chunk_idx, chunk.chunk.len());
                let frame = Frame::data(bytes::Bytes::from(chunk.chunk));
                Some((Ok::<_, std::io::Error>(frame), (iter, ttfb_instant, close_time, false, chunk_idx + 1)))
            } else {
                // All chunks sent, stream ends
                info!("All {} chunks sent, closing stream", chunk_idx);
                None
            }
        },
    );

    let body = StreamBody::new(stream).boxed();
    let response = response_builder.body(body)?;

    Ok(response)
}

use bytes;