use http_body_util::{BodyExt, Full};
use hudsucker::{
    Body, HttpContext, HttpHandler, RequestOrResponse, hyper::Request, hyper::Response,
};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{error, info};

use super::processor::RequestProcessor;
use crate::traits::{RealFileSystem, RealTimeProvider};
use crate::types::{Inventory, Resource};
use std::path::PathBuf;

#[derive(Debug, Clone)]
struct RequestInfo {
    method: String,
    url: String,
    request_start: Instant,
    elapsed_since_start: u64,
}

#[derive(Clone)]
pub struct RecordingHandler {
    shared_inventory: Arc<Mutex<Inventory>>,
    processor: Arc<RequestProcessor<RealFileSystem, RealTimeProvider>>,
    start_time: Arc<Instant>,
    // Connection-based FIFO queues: each client address has its own request queue
    // This handles HTTP/1.1 pipelining and ensures correct request-response pairing per connection
    request_infos: Arc<Mutex<HashMap<SocketAddr, VecDeque<RequestInfo>>>>,
    request_counter: Arc<Mutex<u64>>,
}

impl RecordingHandler {
    pub fn new(inventory: Inventory, inventory_dir: PathBuf) -> Self {
        let processor = Arc::new(RequestProcessor::new(
            inventory_dir,
            Arc::new(RealFileSystem),
            Arc::new(RealTimeProvider::new()),
        ));

        Self {
            shared_inventory: Arc::new(Mutex::new(inventory)),
            processor,
            start_time: Arc::new(Instant::now()),
            request_infos: Arc::new(Mutex::new(HashMap::new())),
            request_counter: Arc::new(Mutex::new(0)),
        }
    }

    pub fn get_inventory(&self) -> Arc<Mutex<Inventory>> {
        self.shared_inventory.clone()
    }
}

impl HttpHandler for RecordingHandler {
    fn handle_request(
        &mut self,
        ctx: &HttpContext,
        req: Request<Body>,
    ) -> impl Future<Output = RequestOrResponse> + Send {
        let method = req.method().clone();
        let uri = req.uri().clone();
        let headers = req.headers().clone();
        let client_addr = ctx.client_addr;

        let start_time = Arc::clone(&self.start_time);
        let request_infos = Arc::clone(&self.request_infos);
        let request_counter = Arc::clone(&self.request_counter);

        async move {
            // Generate unique request ID
            let request_id = {
                let mut counter = request_counter.lock().await;
                *counter += 1;
                *counter
            };

            // Skip CONNECT requests - they are for tunnel establishment, not actual HTTP requests
            if method == "CONNECT" {
                info!("Skipping CONNECT request (tunnel): {}", uri);
                return RequestOrResponse::Request(req);
            }

            info!("Recording request #{}: {} {}", request_id, method, uri);

            // Store request timing
            let request_start = Instant::now();
            let elapsed_since_start = request_start.duration_since(*start_time).as_millis() as u64;

            // Reconstruct full URL (including query parameters)
            let url = if uri.scheme().is_some() {
                uri.to_string()
            } else {
                // Reconstruct from Host header
                if let Some(host) = headers.get("host") {
                    if let Ok(host_str) = host.to_str() {
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

            // Store request information for correlation with response
            // Use connection-based FIFO: push to the back of this client's queue
            {
                let mut infos = request_infos.lock().await;
                infos
                    .entry(client_addr)
                    .or_insert_with(VecDeque::new)
                    .push_back(RequestInfo {
                        method: method.to_string(),
                        url: url.clone(),
                        request_start,
                        elapsed_since_start,
                    });
            }

            RequestOrResponse::Request(req)
        }
    }

    fn handle_response(
        &mut self,
        ctx: &HttpContext,
        res: Response<Body>,
    ) -> impl Future<Output = Response<Body>> + Send {
        let status = res.status();
        let client_addr = ctx.client_addr;

        let start_time = Arc::clone(&self.start_time);
        let request_infos = Arc::clone(&self.request_infos);
        let shared_inventory = Arc::clone(&self.shared_inventory);
        let processor = Arc::clone(&self.processor);

        async move {
            let headers = res.headers().clone();

            // Record TTFB (time to first byte)
            let ttfb_instant = Instant::now();

            info!("Recording response: {}", status);

            let (parts, body) = res.into_parts();

            // Buffer the entire response body
            let body_bytes = match body.collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(e) => {
                    error!("Failed to read response body: {}", e);
                    return Response::from_parts(parts, Body::empty());
                }
            };

            // Find matching request info using connection-based FIFO
            // Pop from the front of this client's queue (oldest request first)
            let request_info = {
                let mut infos = request_infos.lock().await;
                if let Some(queue) = infos.get_mut(&client_addr) {
                    queue.pop_front()
                } else {
                    None
                }
            };

            let (method_str, url, ttfb_ms, download_end_ms) = if let Some(info) = request_info {
                // Calculate TTFB relative to request start (pure TTFB duration)
                let ttfb = ttfb_instant.duration_since(info.request_start).as_millis() as u64;
                // Store only the pure TTFB, not the absolute time
                let ttfb_ms = ttfb;

                // Calculate download end time relative to request start (not proxy start)
                let download_end = Instant::now();
                let download_end_ms =
                    download_end.duration_since(info.request_start).as_millis() as u64;

                info!(
                    "Matched response with request: {} {} (TTFB: {}ms, download_end: {}ms, request offset: {}ms)",
                    info.method, info.url, ttfb, download_end_ms, info.elapsed_since_start
                );

                (info.method, info.url, ttfb_ms, download_end_ms)
            } else {
                // Fallback - this should rarely happen with connection-based FIFO
                error!("No matching request info found for client: {}", client_addr);
                let elapsed = ttfb_instant.duration_since(*start_time).as_millis() as u64;
                let download_end = Instant::now();
                let download_end_elapsed =
                    download_end.duration_since(*start_time).as_millis() as u64;
                (
                    "GET".to_string(),
                    "unknown".to_string(),
                    elapsed,
                    download_end_elapsed,
                )
            };

            // Create resource
            let mut resource = Resource::new(method_str, url.clone());
            resource.status_code = Some(status.as_u16());
            resource.ttfb_ms = ttfb_ms;
            resource.download_end_ms = Some(download_end_ms);

            // Store response headers
            // Multiple headers with the same name (like Set-Cookie) are collected into arrays
            let mut resource_headers = std::collections::HashMap::new();
            for (name, value) in headers.iter() {
                if let Ok(value_str) = value.to_str() {
                    let header_name = name.to_string();
                    let value_string = value_str.to_string();

                    resource_headers
                        .entry(header_name)
                        .and_modify(|existing| {
                            // If header already exists, convert to Multiple or append to existing Multiple
                            match existing {
                                crate::types::HeaderValue::Single(first) => {
                                    *existing = crate::types::HeaderValue::Multiple(vec![
                                        first.clone(),
                                        value_string.clone(),
                                    ]);
                                }
                                crate::types::HeaderValue::Multiple(values) => {
                                    values.push(value_string.clone());
                                }
                            }
                        })
                        .or_insert_with(|| crate::types::HeaderValue::Single(value_string));
                }
            }
            resource.raw_headers = Some(resource_headers);

            // Detect content-encoding
            if let Some(encoding_header) = headers.get("content-encoding") {
                if let Ok(encoding_str) = encoding_header.to_str() {
                    if let Ok(encoding) = encoding_str.parse::<crate::types::ContentEncodingType>()
                    {
                        resource.content_encoding = Some(encoding);
                    }
                }
            }

            // Process response body
            let content_type = headers.get("content-type").and_then(|v| v.to_str().ok());
            if let Err(e) = processor
                .process_response_body(&mut resource, &body_bytes, content_type)
                .await
            {
                error!("Failed to process response body: {}", e);
            }

            // Add resource to inventory
            {
                let mut inventory = shared_inventory.lock().await;
                inventory.resources.push(resource);
            }

            // Return response with the buffered body
            Response::from_parts(parts, Body::from(Full::new(body_bytes)))
        }
    }
}
