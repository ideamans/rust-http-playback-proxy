use anyhow::Result;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper::body::Incoming;
use http_body_util::Full;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{info, error, debug};

use crate::types::{Inventory, Resource};
use serde::Serialize;
use super::processor::RequestProcessor;

pub async fn start_recording_proxy(
    port: u16,
    inventory: Inventory,
    inventory_dir: PathBuf,
) -> Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    
    info!("Recording proxy listening on {}", addr);
    
    let shared_inventory = Arc::new(Mutex::new(inventory));
    let processor = Arc::new(RequestProcessor::new(
        inventory_dir.clone(),
        Arc::new(RealFileSystem),
        Arc::new(RealTimeProvider::new())
    ));
    let start_time = Arc::new(Instant::now());

    // Setup Ctrl+C handler
    let shared_inventory_clone = shared_inventory.clone();
    let inventory_dir_clone = inventory_dir.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        info!("Received Ctrl+C, saving inventory...");
        
        let inventory = shared_inventory_clone.lock().await;
        if let Err(e) = save_inventory(&*inventory, &inventory_dir_clone).await {
            error!("Failed to save inventory: {}", e);
        } else {
            info!("Inventory saved successfully");
        }
        
        std::process::exit(0);
    });

    loop {
        let (stream, _) = listener.accept().await?;
        let shared_inventory = shared_inventory.clone();
        let processor = processor.clone();
        let start_time = start_time.clone();

        tokio::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(
                    TokioIo::new(stream),
                    service_fn(|req| {
                        handle_request(
                            req,
                            shared_inventory.clone(),
                            processor.clone(),
                            start_time.clone(),
                        )
                    }),
                )
                .await
            {
                error!("Error serving connection: {:?}", err);
            }
        });
    }
}

async fn handle_request(
    req: Request<Incoming>,
    shared_inventory: Arc<Mutex<Inventory>>,
    processor: Arc<RequestProcessor<RealFileSystem, RealTimeProvider>>,
    start_time: Arc<Instant>,
) -> Result<Response<Full<bytes::Bytes>>, hyper::Error> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();
    
    debug!("Handling request: {} {}", method, uri);
    info!("Full request details - Method: {}, URI: {}, Headers: {:?}", method, uri, headers);

    let request_start = Instant::now();
    let elapsed_since_start = request_start.duration_since(*start_time).as_millis() as u64;

    match handle_proxy_request(req, &processor, elapsed_since_start).await {
        Ok((response, resource)) => {
            // Add resource to inventory
            let mut inventory = shared_inventory.lock().await;
            inventory.resources.push(resource);
            
            Ok(response)
        }
        Err(e) => {
            error!("Error handling proxy request: {}", e);
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(bytes::Bytes::from(format!("Proxy error: {}", e))))
                .unwrap())
        }
    }
}

async fn handle_proxy_request(
    req: Request<Incoming>,
    processor: &RequestProcessor<RealFileSystem, RealTimeProvider>,
    elapsed_since_start: u64,
) -> Result<(Response<Full<bytes::Bytes>>, Resource)> {
    let method = req.method().to_string();
    let uri = req.uri();
    let headers = req.headers();
    
    info!("Received proxy request: {} {}", method, uri);
    info!("URI components - scheme: {:?}, host: {:?}, path: {:?}", 
          uri.scheme(), uri.host(), uri.path());
    info!("Request headers: {:?}", headers);
    
    // For HTTP proxy, the URI should be the full URL
    // If it's just a path, we need more context
    let url = if uri.scheme().is_some() && uri.host().is_some() {
        // This is a full URL (proxy request)
        uri.to_string()
    } else {
        // This is just a path, we need more context
        error!("Invalid proxy request: URI = {}, scheme = {:?}, host = {:?}", 
               uri, uri.scheme(), uri.host());
        return Err(anyhow::anyhow!("Invalid proxy request: missing host information: {}", uri));
    };
    
    debug!("Forwarding request: {} {}", method, url);
    info!("Proxy forwarding: {} {} (URI: {})", method, url, uri);
    
    // Simple HTTP client to forward requests
    let client = reqwest::Client::new();
    let request_start = std::time::Instant::now();
    
    let response_result = client
        .request(reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET), &url)
        .send()
        .await;
    
    match response_result {
        Ok(response) => {
            let status_code = response.status().as_u16();
            let headers = response.headers().clone();
            let body_bytes = response.bytes().await.unwrap_or_default();
            
            let ttfb = request_start.elapsed().as_millis() as u64;
            
            let mut resource = Resource::new(method, url);
            resource.ttfb_ms = elapsed_since_start + ttfb;
            resource.status_code = Some(status_code);
            
            // Convert headers
            let mut resource_headers = std::collections::HashMap::new();
            for (name, value) in headers.iter() {
                if let Ok(value_str) = value.to_str() {
                    resource_headers.insert(name.to_string(), value_str.to_string());
                }
            }
            resource.raw_headers = Some(resource_headers);
            
            // Process the response body using the processor
            let content_type = headers.get("content-type")
                .and_then(|h| h.to_str().ok());
            
            if let Err(e) = processor.process_response_body(&mut resource, &body_bytes, content_type).await {
                error!("Failed to process response body: {}", e);
            }
            
            let proxy_response = Response::builder()
                .status(StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK))
                .body(Full::new(bytes::Bytes::from(body_bytes.clone())))
                .unwrap();

            Ok((proxy_response, resource))
        }
        Err(e) => {
            error!("HTTP client error: {}", e);
            let mut resource = Resource::new(method, url);
            resource.ttfb_ms = elapsed_since_start;
            resource.status_code = Some(500);
            resource.error_message = Some(format!("Proxy error: {}", e));
            
            let error_response = Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(bytes::Bytes::from(format!("Proxy error: {}", e))))
                .unwrap();

            Ok((error_response, resource))
        }
    }
}

use crate::traits::{FileSystem, RealFileSystem, RealTimeProvider};

pub async fn save_inventory(inventory: &Inventory, inventory_dir: &PathBuf) -> Result<()> {
    let file_system = Arc::new(RealFileSystem);
    save_inventory_with_fs(inventory, inventory_dir, file_system).await
}

pub async fn save_inventory_with_fs<F: FileSystem>(
    inventory: &Inventory,
    inventory_dir: &PathBuf,
    file_system: Arc<F>,
) -> Result<()> {
    file_system.create_dir_all(inventory_dir).await?;
    
    let inventory_path = inventory_dir.join("inventory.json");
    // 2スペースインデントで整形
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    inventory.serialize(&mut ser)?;
    let inventory_json = String::from_utf8(buf)?;
    
    file_system.write_string(&inventory_path, &inventory_json).await?;
    
    Ok(())
}