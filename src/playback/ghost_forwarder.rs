//! Ghost Server Forwarding Handler
//!
//! A simple HTTP handler that forwards requests to the Ghost Server
//! while preserving the Host header for virtual host matching.
//! Uses hyper directly instead of reqwest to preserve streaming timing.

use bytes::Bytes;
use futures::TryStreamExt;
use http_body_util::Empty;
use hudsucker::{
    Body, HttpContext, HttpHandler, RequestOrResponse,
    hyper::{Request, Response, StatusCode},
};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use std::future::Future;
use tracing::{error, info};

/// Simple forwarding handler that sends all requests to Ghost Server
/// Uses hyper directly for minimal buffering to preserve timing
#[derive(Clone)]
pub struct GhostForwarder {
    /// Ghost Server port
    ghost_port: u16,
    /// Hyper client for forwarding (minimal buffering)
    client: Client<HttpConnector, Empty<Bytes>>,
}

impl GhostForwarder {
    pub fn new(ghost_port: u16) -> Self {
        let connector = HttpConnector::new();
        // Use HTTP/1.1 only for Ghost Server (localhost)
        let client = Client::builder(TokioExecutor::new()).build(connector);

        Self { ghost_port, client }
    }
}

impl HttpHandler for GhostForwarder {
    fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        req: Request<Body>,
    ) -> impl Future<Output = RequestOrResponse> + Send {
        let ghost_port = self.ghost_port;
        let client = self.client.clone();

        async move {
            let method = req.method().clone();
            let uri = req.uri().clone();
            let headers = req.headers().clone();

            // Skip CONNECT requests - they are for tunnel establishment
            if method == hyper::Method::CONNECT {
                info!("Skipping CONNECT request (tunnel): {}", uri);
                return RequestOrResponse::Request(req);
            }

            // Extract host from multiple sources (HTTP/2 and HTTP/1.1 compatible)
            // Priority: 1. :authority pseudo-header, 2. Host header, 3. URI authority
            let host: String = if let Some(authority) = headers
                .get(":authority")
                .and_then(|h| h.to_str().ok())
                .filter(|s| !s.is_empty())
            {
                // HTTP/2 :authority pseudo-header (strip port if present)
                authority.split(':').next().unwrap_or(authority).to_string()
            } else if let Some(host_header) = headers
                .get("host")
                .and_then(|h| h.to_str().ok())
                .filter(|s| !s.is_empty())
            {
                // HTTP/1.1 Host header (strip port if present)
                host_header
                    .split(':')
                    .next()
                    .unwrap_or(host_header)
                    .to_string()
            } else if let Some(uri_host) = uri.host() {
                // Fallback: Extract from URI (for HTTP/2 tunneled requests with full URL)
                uri_host.to_string()
            } else if let Some(authority) = uri.authority() {
                // Fallback: Use URI authority (may include port)
                authority.host().to_string()
            } else {
                error!("No valid host in request: {} {}", method, uri);
                let error_response = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from("Missing host information in request"))
                    .unwrap();
                return RequestOrResponse::Response(error_response);
            };

            let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

            info!(
                "Forwarding to Ghost Server: {} {} (Host: {})",
                method, path_and_query, host
            );

            // Build the forwarding URI to Ghost Server
            let ghost_uri = format!("http://127.0.0.1:{}{}", ghost_port, path_and_query)
                .parse::<hyper::Uri>()
                .expect("Failed to parse Ghost Server URI");

            // Build hyper request
            let mut request_builder = Request::builder().method(method.clone()).uri(ghost_uri);

            // Forward essential headers (skip Host/:authority - we'll set X-Original-Host)
            for (name, value) in headers.iter() {
                let name_str = name.as_str().to_lowercase();

                // Skip hop-by-hop headers, Host, and :authority (will be set via X-Original-Host)
                if matches!(
                    name_str.as_str(),
                    "host"
                        | ":authority"
                        | "connection"
                        | "keep-alive"
                        | "proxy-authenticate"
                        | "proxy-authorization"
                        | "te"
                        | "trailer"
                        | "transfer-encoding"
                        | "upgrade"
                        | "proxy-connection"
                ) {
                    continue;
                }

                request_builder = request_builder.header(name.clone(), value.clone());
            }

            // Send original host as X-Original-Host custom header
            request_builder = request_builder.header("X-Original-Host", &host);

            // Build request with empty body (we only support GET/HEAD in playback)
            let ghost_request = match request_builder.body(Empty::<Bytes>::new()) {
                Ok(req) => req,
                Err(e) => {
                    error!("Failed to build request: {}", e);
                    let error_response = Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from(format!("Request build error: {}", e)))
                        .unwrap();
                    return RequestOrResponse::Response(error_response);
                }
            };

            // Execute the request to Ghost Server
            match client.request(ghost_request).await {
                Ok(ghost_response) => {
                    // Convert hyper response to hudsucker response
                    let status = ghost_response.status();
                    let headers = ghost_response.headers().clone();

                    info!("Ghost Server responded with status: {}", status);

                    // Build response
                    let mut response_builder = Response::builder().status(status.as_u16());

                    // Copy response headers
                    for (name, value) in headers.iter() {
                        let name_str = name.as_str().to_lowercase();

                        // Skip hop-by-hop headers
                        if matches!(
                            name_str.as_str(),
                            "connection" | "keep-alive" | "transfer-encoding" | "proxy-connection"
                        ) {
                            continue;
                        }

                        response_builder = response_builder.header(name.clone(), value.clone());
                    }

                    // Stream the body from Ghost Server directly
                    // Convert hyper Incoming to a stream of bytes
                    use http_body_util::BodyExt;
                    let body_stream = ghost_response
                        .into_body()
                        .into_data_stream()
                        .map_err(|e| std::io::Error::other(format!("hyper stream error: {}", e)));
                    let body = Body::from_stream(body_stream);

                    match response_builder.body(body) {
                        Ok(response) => RequestOrResponse::Response(response),
                        Err(e) => {
                            error!("Failed to build response: {}", e);
                            let error_response = Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::from(format!("Response build error: {}", e)))
                                .unwrap();
                            RequestOrResponse::Response(error_response)
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to forward request to Ghost Server: {}", e);
                    let error_response = Response::builder()
                        .status(StatusCode::BAD_GATEWAY)
                        .body(Body::from(format!("Ghost Server error: {}", e)))
                        .unwrap();
                    RequestOrResponse::Response(error_response)
                }
            }
        }
    }

    async fn handle_response(&mut self, _ctx: &HttpContext, res: Response<Body>) -> Response<Body> {
        // Pass through responses unchanged
        res
    }
}
