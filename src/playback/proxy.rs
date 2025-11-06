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
use tracing::{info, error, debug};
use hyper::body::Frame;
use futures::stream;

use crate::types::Transaction;

pub async fn start_playback_proxy(port: u16, transactions: Vec<Transaction>) -> Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    
    info!("Playback proxy listening on {}", addr);
    
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
    let uri = req.uri().to_string();

    debug!("Handling playback request: {} {}", method, uri);

    // Find matching transaction
    let transaction = transactions
        .iter()
        .find(|t| t.method == method && t.url == uri)
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
            info!("No transaction found for: {} {}", method, uri);
            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(
                    http_body_util::Full::new(bytes::Bytes::from("Resource not found in playback data"))
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                        .boxed()
                )
                .unwrap())
        }
    }
}

async fn serve_transaction(
    transaction: Transaction,
    start_time: Arc<Instant>,
) -> Result<Response<http_body_util::combinators::BoxBody<bytes::Bytes, std::io::Error>>> {
    use http_body_util::BodyExt;

    let request_start = Instant::now();
    let elapsed_since_start = request_start.duration_since(*start_time).as_millis() as u64;

    // Wait for TTFB
    if transaction.ttfb > elapsed_since_start {
        let wait_time = transaction.ttfb - elapsed_since_start;
        tokio::time::sleep(Duration::from_millis(wait_time)).await;
    }

    // If there's an error message, return error response
    if let Some(error_msg) = &transaction.error_message {
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

    // Add headers
    if let Some(headers) = &transaction.raw_headers {
        for (key, value) in headers {
            response_builder = response_builder.header(key, value);
        }
    }

    // Create streaming body with timing control
    let chunks = transaction.chunks.clone();
    let target_close_time = transaction.target_close_time;
    let request_instant = Instant::now();

    let stream = stream::unfold(
        (chunks.into_iter(), request_instant, target_close_time, false),
        |(mut iter, req_instant, close_time, is_done)| async move {
            if is_done {
                return None;
            }

            if let Some(chunk) = iter.next() {
                // Check current elapsed time
                let elapsed = req_instant.elapsed().as_millis() as u64;

                // Only wait if we're ahead of schedule
                // If we're behind (elapsed > target_time), send immediately to catch up
                if chunk.target_time > elapsed {
                    let wait_time = chunk.target_time - elapsed;
                    debug!("Waiting {}ms before sending chunk (target: {}ms, elapsed: {}ms)",
                           wait_time, chunk.target_time, elapsed);
                    tokio::time::sleep(Duration::from_millis(wait_time)).await;
                } else if chunk.target_time < elapsed {
                    // We're behind schedule - log it but send immediately
                    let behind_ms = elapsed - chunk.target_time;
                    debug!("Behind schedule by {}ms, sending chunk immediately (target: {}ms, elapsed: {}ms)",
                           behind_ms, chunk.target_time, elapsed);
                }

                // Send chunk
                let frame = Frame::data(bytes::Bytes::from(chunk.chunk));
                Some((Ok::<_, std::io::Error>(frame), (iter, req_instant, close_time, false)))
            } else {
                // All chunks sent, wait until target_close_time before closing
                let elapsed = req_instant.elapsed().as_millis() as u64;
                if close_time > elapsed {
                    let wait_time = close_time - elapsed;
                    debug!("All chunks sent, waiting {}ms until target_close_time", wait_time);
                    tokio::time::sleep(Duration::from_millis(wait_time)).await;
                } else {
                    let behind_ms = elapsed - close_time;
                    debug!("Behind target_close_time by {}ms, closing immediately", behind_ms);
                }
                None
            }
        },
    );

    let body = StreamBody::new(stream).boxed();
    let response = response_builder.body(body)?;

    Ok(response)
}

use bytes;