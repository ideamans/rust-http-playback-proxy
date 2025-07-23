use anyhow::Result;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper::body::Incoming;
use http_body_util::Full;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use std::sync::Arc;
use std::time::{Instant, Duration};
use tracing::{info, error, debug};

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
) -> Result<Response<Full<bytes::Bytes>>, hyper::Error> {
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
                        .body(Full::new(bytes::Bytes::from(format!("Transaction error: {}", e))))
                        .unwrap())
                }
            }
        }
        None => {
            info!("No transaction found for: {} {}", method, uri);
            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Full::new(bytes::Bytes::from("Resource not found in playback data")))
                .unwrap())
        }
    }
}

async fn serve_transaction(
    transaction: Transaction,
    start_time: Arc<Instant>,
) -> Result<Response<Full<bytes::Bytes>>> {
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
            .body(Full::new(bytes::Bytes::from(error_msg.clone())))?);
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

    // Collect all chunks into a single body
    // In a real implementation, we would stream chunks with proper timing
    let mut body_data = Vec::new();
    for chunk in &transaction.chunks {
        body_data.extend_from_slice(&chunk.chunk);
    }

    let response = response_builder
        .body(Full::new(bytes::Bytes::from(body_data)))?;

    Ok(response)
}

use bytes;