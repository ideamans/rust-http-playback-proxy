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
    let _headers = req.headers().clone();
    
    debug!("Handling request: {} {}", method, uri);

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
    _processor: &RequestProcessor<RealFileSystem, RealTimeProvider>,
    elapsed_since_start: u64,
) -> Result<(Response<Full<bytes::Bytes>>, Resource)> {
    let method = req.method().to_string();
    let url = req.uri().to_string();
    
    // For now, return a simple response
    // TODO: Implement actual HTTP proxying
    let mut resource = Resource::new(method, url);
    resource.ttfb_ms = elapsed_since_start;
    resource.status_code = Some(200);
    
    let response_body = "Recording mode - this would be the actual response";
    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/plain")
        .body(Full::new(bytes::Bytes::from(response_body)))
        .unwrap();

    Ok((response, resource))
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
    let inventory_json = serde_json::to_string_pretty(inventory)?;
    
    file_system.write_string(&inventory_path, &inventory_json).await?;
    
    Ok(())
}