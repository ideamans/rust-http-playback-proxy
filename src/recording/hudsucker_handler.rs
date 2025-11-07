use http_body_util::{BodyExt, Full};
use hudsucker::{
    hyper::Request, hyper::Response, HttpContext, HttpHandler,
    RequestOrResponse, Body,
};
use std::sync::Arc;
use std::time::Instant;
use std::future::Future;
use tokio::sync::Mutex;
use tracing::{error, info};
use std::collections::VecDeque;

use crate::types::{Inventory, Resource};
use super::processor::RequestProcessor;
use crate::traits::{RealFileSystem, RealTimeProvider};
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
    request_infos: Arc<Mutex<VecDeque<(String, RequestInfo)>>>,
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
            request_infos: Arc::new(Mutex::new(VecDeque::new())),
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
        _ctx: &HttpContext,
        req: Request<Body>,
    ) -> impl Future<Output = RequestOrResponse> + Send {
        let method = req.method().clone();
        let uri = req.uri().clone();
        let headers = req.headers().clone();

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
            // Use VecDeque to maintain FIFO order
            {
                let mut infos = request_infos.lock().await;
                infos.push_back((request_id.to_string(), RequestInfo {
                    method: method.to_string(),
                    url: url.clone(),
                    request_start,
                    elapsed_since_start,
                }));
            }

            // DON'T modify request - just pass it through unchanged
            // Pass the request through
            RequestOrResponse::Request(req)
        }
    }

    fn handle_response(
        &mut self,
        _ctx: &HttpContext,
        res: Response<Body>,
    ) -> impl Future<Output = Response<Body>> + Send {
        let status = res.status();

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

            // Find matching request info (FIFO - get the oldest/first request)
            let request_info = {
                let mut infos = request_infos.lock().await;
                // Pop the first entry (oldest request) from the queue
                infos.pop_front().map(|(_, info)| info)
            };

            let (method_str, url, ttfb_ms) = if let Some(info) = request_info {
                // Calculate TTFB relative to request start (pure TTFB duration)
                let ttfb = ttfb_instant.duration_since(info.request_start).as_millis() as u64;
                // Store only the pure TTFB, not the absolute time
                let ttfb_ms = ttfb;

                info!("Matched response with request: {} {} (TTFB: {}ms, request offset: {}ms)",
                      info.method, info.url, ttfb, info.elapsed_since_start);

                (info.method, info.url, ttfb_ms)
            } else {
                // Fallback
                error!("No matching request info found for response");
                let elapsed = ttfb_instant.duration_since(*start_time).as_millis() as u64;
                ("GET".to_string(), "unknown".to_string(), elapsed)
            };

            // Calculate download end time
            let download_end = Instant::now();
            let download_end_ms = download_end.duration_since(*start_time).as_millis() as u64;

            // Create resource
            let mut resource = Resource::new(method_str, url.clone());
            resource.status_code = Some(status.as_u16());
            resource.ttfb_ms = ttfb_ms;
            resource.download_end_ms = Some(download_end_ms);

            // Store response headers
            let mut resource_headers = std::collections::HashMap::new();
            for (name, value) in headers.iter() {
                if let Ok(value_str) = value.to_str() {
                    resource_headers.insert(name.to_string(), value_str.to_string());
                }
            }
            resource.raw_headers = Some(resource_headers);

            // Detect content-encoding
            if let Some(encoding_header) = headers.get("content-encoding") {
                if let Ok(encoding_str) = encoding_header.to_str() {
                    if let Ok(encoding) = encoding_str.parse::<crate::types::ContentEncodingType>() {
                        resource.content_encoding = Some(encoding);
                    }
                }
            }

            // Process response body
            let content_type = headers.get("content-type").and_then(|v| v.to_str().ok());
            if let Err(e) = processor.process_response_body(&mut resource, &body_bytes, content_type).await {
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
