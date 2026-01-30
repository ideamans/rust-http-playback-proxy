//! Ghost Server Forwarding Handler
//!
//! Forwards requests to the appropriate Ghost Server based on host and scheme.
//! - HTTPS origins are forwarded via HTTPS to HTTPS Ghost Servers
//! - HTTP origins are forwarded via HTTP to HTTP Ghost Servers

use bytes::Bytes;
use futures::TryStreamExt;
use http_body_util::Empty;
use hudsucker::{
    Body, HttpContext, HttpHandler, RequestOrResponse,
    hyper::{Request, Response, StatusCode},
};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::ghost_server::pool::RoutingEntry;

/// Custom certificate verifier that accepts all certificates (for self-signed Ghost Server certs)
#[derive(Debug)]
struct AcceptAllCertVerifier;

impl ServerCertVerifier for AcceptAllCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

/// Forwarding handler that routes requests to appropriate Ghost Servers (HTTP or HTTPS)
#[derive(Clone)]
pub struct GhostForwarder {
    /// Routing table: "scheme://host" -> RoutingEntry
    routing_table: Arc<HashMap<String, RoutingEntry>>,
    /// HTTPS client (accepts self-signed certs)
    https_client: Client<
        hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
        Empty<Bytes>,
    >,
    /// HTTP client (plain, no TLS)
    http_client: Client<HttpConnector, Empty<Bytes>>,
    /// Whether to forward unmatched requests to real servers
    passthrough: bool,
}

impl GhostForwarder {
    pub fn new(routing_table: HashMap<String, RoutingEntry>, passthrough: bool) -> Self {
        // Create rustls config that accepts self-signed certificates
        let tls_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAllCertVerifier))
            .with_no_client_auth();

        // Create HTTPS connector
        let https_connector = HttpsConnectorBuilder::new()
            .with_tls_config(tls_config)
            .https_or_http()
            .enable_http1()
            .build();

        let https_client = Client::builder(TokioExecutor::new()).build(https_connector);

        // Create plain HTTP connector
        let http_connector = HttpConnector::new();
        let http_client = Client::builder(TokioExecutor::new()).build(http_connector);

        Self {
            routing_table: Arc::new(routing_table),
            https_client,
            http_client,
            passthrough,
        }
    }
}

impl HttpHandler for GhostForwarder {
    fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        req: Request<Body>,
    ) -> impl Future<Output = RequestOrResponse> + Send {
        let routing_table = self.routing_table.clone();
        let https_client = self.https_client.clone();
        let http_client = self.http_client.clone();
        let passthrough = self.passthrough;

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
            // host_with_port preserves the original host:port for passthrough
            let host_with_port: String = if let Some(authority) = headers
                .get(":authority")
                .and_then(|h| h.to_str().ok())
                .filter(|s| !s.is_empty())
            {
                authority.to_string()
            } else if let Some(host_header) = headers
                .get("host")
                .and_then(|h| h.to_str().ok())
                .filter(|s| !s.is_empty())
            {
                host_header.to_string()
            } else if let Some(authority) = uri.authority() {
                authority.as_str().to_string()
            } else if let Some(uri_host) = uri.host() {
                uri_host.to_string()
            } else {
                error!("No valid host in request: {} {}", method, uri);
                let error_response = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from("Missing host information in request"))
                    .unwrap();
                return RequestOrResponse::Response(error_response);
            };

            // Strip port for routing table lookup
            let host = host_with_port
                .split(':')
                .next()
                .unwrap_or(&host_with_port)
                .to_string();

            // Determine scheme from URI (HTTPS by default for proxied requests)
            let scheme = uri.scheme_str().unwrap_or("https");
            let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

            // Look up the Ghost Server for this origin
            let routing_key = format!("{}://{}", scheme, host);
            let routing_entry = routing_table.get(&routing_key).cloned();

            let (ghost_port, use_https) = match routing_entry {
                Some(entry) => (entry.port, entry.is_https),
                None => {
                    // Try the opposite scheme as fallback
                    let alt_scheme = if scheme == "https" { "http" } else { "https" };
                    let alt_key = format!("{}://{}", alt_scheme, host);

                    if let Some(entry) = routing_table.get(&alt_key) {
                        warn!(
                            "No Ghost Server for {}, using {} instead",
                            routing_key, alt_key
                        );
                        (entry.port, entry.is_https)
                    } else if passthrough {
                        // Passthrough: forward to real server
                        info!(
                            "No Ghost Server for {}, passthrough to real server",
                            routing_key
                        );
                        return forward_to_real_server(
                            &method,
                            scheme,
                            &host_with_port,
                            path_and_query,
                            &headers,
                            &https_client,
                            &http_client,
                        )
                        .await;
                    } else {
                        error!("No Ghost Server available for host: {}", host);
                        let error_response = Response::builder()
                            .status(StatusCode::BAD_GATEWAY)
                            .body(Body::from(format!("No Ghost Server for: {}", routing_key)))
                            .unwrap();
                        return RequestOrResponse::Response(error_response);
                    }
                }
            };

            let protocol = if use_https { "HTTPS" } else { "HTTP" };

            info!(
                "Forwarding to {} Ghost Server: {} {} (Host: {} -> port {})",
                protocol, method, path_and_query, host, ghost_port
            );

            // Build the forwarding URI
            let ghost_scheme = if use_https { "https" } else { "http" };
            let ghost_uri = format!(
                "{}://127.0.0.1:{}{}",
                ghost_scheme, ghost_port, path_and_query
            )
            .parse::<hyper::Uri>()
            .expect("Failed to parse Ghost Server URI");

            // Build hyper request
            let mut request_builder = Request::builder().method(method.clone()).uri(ghost_uri);

            // Forward essential headers
            for (name, value) in headers.iter() {
                let name_str = name.as_str().to_lowercase();

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

            // Build request with empty body
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

            // Execute the request to Ghost Server (HTTPS or HTTP)
            let result = if use_https {
                https_client.request(ghost_request).await
            } else {
                http_client.request(ghost_request).await
            };

            match result {
                Ok(ghost_response) => {
                    let status = ghost_response.status();
                    let resp_headers = ghost_response.headers().clone();

                    info!("Ghost Server responded with status: {}", status);

                    // Check for Ghost Server miss (X-Ghost-Miss header)
                    if passthrough
                        && resp_headers
                            .get("x-ghost-miss")
                            .and_then(|v| v.to_str().ok())
                            == Some("true")
                    {
                        info!(
                            "Ghost Server miss detected, passthrough to real server: {} {}",
                            method, path_and_query
                        );
                        return forward_to_real_server(
                            &method,
                            scheme,
                            &host_with_port,
                            path_and_query,
                            &headers,
                            &https_client,
                            &http_client,
                        )
                        .await;
                    }

                    let mut response_builder = Response::builder().status(status.as_u16());

                    for (name, value) in resp_headers.iter() {
                        let name_str = name.as_str().to_lowercase();

                        if matches!(
                            name_str.as_str(),
                            "connection" | "keep-alive" | "transfer-encoding" | "proxy-connection"
                        ) {
                            continue;
                        }

                        response_builder = response_builder.header(name.clone(), value.clone());
                    }

                    // Stream the body from Ghost Server directly
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
        res
    }
}

/// Forward a request to the real upstream server (passthrough)
async fn forward_to_real_server(
    method: &hyper::Method,
    scheme: &str,
    host_with_port: &str,
    path_and_query: &str,
    original_headers: &hyper::HeaderMap,
    https_client: &Client<
        hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
        Empty<Bytes>,
    >,
    http_client: &Client<HttpConnector, Empty<Bytes>>,
) -> RequestOrResponse {
    let real_uri = format!("{}://{}{}", scheme, host_with_port, path_and_query);
    info!(
        "Passthrough: forwarding to real server: {} {}",
        method, real_uri
    );

    let parsed_uri = match real_uri.parse::<hyper::Uri>() {
        Ok(uri) => uri,
        Err(e) => {
            error!("Failed to parse passthrough URI: {}", e);
            let error_response = Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from(format!("Passthrough URI parse error: {}", e)))
                .unwrap();
            return RequestOrResponse::Response(error_response);
        }
    };

    let mut request_builder = Request::builder().method(method.clone()).uri(parsed_uri);

    // Forward headers, skipping hop-by-hop headers
    for (name, value) in original_headers.iter() {
        let name_str = name.as_str().to_lowercase();

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

    let real_request = match request_builder.body(Empty::<Bytes>::new()) {
        Ok(req) => req,
        Err(e) => {
            error!("Failed to build passthrough request: {}", e);
            let error_response = Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(format!(
                    "Passthrough request build error: {}",
                    e
                )))
                .unwrap();
            return RequestOrResponse::Response(error_response);
        }
    };

    let use_https = scheme == "https";
    let result = if use_https {
        https_client.request(real_request).await
    } else {
        http_client.request(real_request).await
    };

    match result {
        Ok(real_response) => {
            let status = real_response.status();
            let resp_headers = real_response.headers().clone();

            info!("Passthrough: real server responded with status: {}", status);

            let mut response_builder = Response::builder().status(status.as_u16());

            for (name, value) in resp_headers.iter() {
                let name_str = name.as_str().to_lowercase();

                if matches!(
                    name_str.as_str(),
                    "connection" | "keep-alive" | "transfer-encoding" | "proxy-connection"
                ) {
                    continue;
                }

                response_builder = response_builder.header(name.clone(), value.clone());
            }

            use http_body_util::BodyExt;
            let body_stream = real_response
                .into_body()
                .into_data_stream()
                .map_err(|e| std::io::Error::other(format!("passthrough stream error: {}", e)));
            let body = Body::from_stream(body_stream);

            match response_builder.body(body) {
                Ok(response) => RequestOrResponse::Response(response),
                Err(e) => {
                    error!("Failed to build passthrough response: {}", e);
                    let error_response = Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from(format!(
                            "Passthrough response build error: {}",
                            e
                        )))
                        .unwrap();
                    RequestOrResponse::Response(error_response)
                }
            }
        }
        Err(e) => {
            error!("Passthrough: failed to forward to real server: {}", e);
            let error_response = Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from(format!("Passthrough error: {}", e)))
                .unwrap();
            RequestOrResponse::Response(error_response)
        }
    }
}
