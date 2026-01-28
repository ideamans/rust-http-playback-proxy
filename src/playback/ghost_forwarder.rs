//! Ghost Server Forwarding Handler
//!
//! Forwards requests to the appropriate Ghost HTTPS Server based on host.
//! Each domain has its own Ghost Server, providing realistic TLS handshake simulation.

use bytes::Bytes;
use futures::TryStreamExt;
use http_body_util::Empty;
use hudsucker::{
    Body, HttpContext, HttpHandler, RequestOrResponse,
    hyper::{Request, Response, StatusCode},
};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tracing::{error, info, warn};

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

/// Forwarding handler that routes requests to domain-specific Ghost HTTPS Servers
#[derive(Clone)]
pub struct GhostForwarder {
    /// Routing table: host -> port
    routing_table: Arc<HashMap<String, u16>>,
    /// Fallback port (for unknown hosts)
    fallback_port: Option<u16>,
    /// HTTPS client (accepts self-signed certs)
    client: Client<
        hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
        Empty<Bytes>,
    >,
}

impl GhostForwarder {
    pub fn new(routing_table: HashMap<String, u16>) -> Self {
        // Find a fallback port (use the first one if available)
        let fallback_port = routing_table.values().next().copied();

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

        let client = Client::builder(TokioExecutor::new()).build(https_connector);

        Self {
            routing_table: Arc::new(routing_table),
            fallback_port,
            client,
        }
    }

    /// For backward compatibility - single Ghost Server
    #[allow(dead_code)]
    pub fn new_single(ghost_port: u16) -> Self {
        let routing_table = HashMap::new();
        // Empty routing table with fallback
        Self {
            routing_table: Arc::new(routing_table),
            fallback_port: Some(ghost_port),
            client: {
                let tls_config = rustls::ClientConfig::builder()
                    .dangerous()
                    .with_custom_certificate_verifier(Arc::new(AcceptAllCertVerifier))
                    .with_no_client_auth();
                let https_connector = HttpsConnectorBuilder::new()
                    .with_tls_config(tls_config)
                    .https_or_http()
                    .enable_http1()
                    .build();
                Client::builder(TokioExecutor::new()).build(https_connector)
            },
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
        let fallback_port = self.fallback_port;
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
            let host: String = if let Some(authority) = headers
                .get(":authority")
                .and_then(|h| h.to_str().ok())
                .filter(|s| !s.is_empty())
            {
                authority.split(':').next().unwrap_or(authority).to_string()
            } else if let Some(host_header) = headers
                .get("host")
                .and_then(|h| h.to_str().ok())
                .filter(|s| !s.is_empty())
            {
                host_header
                    .split(':')
                    .next()
                    .unwrap_or(host_header)
                    .to_string()
            } else if let Some(uri_host) = uri.host() {
                uri_host.to_string()
            } else if let Some(authority) = uri.authority() {
                authority.host().to_string()
            } else {
                error!("No valid host in request: {} {}", method, uri);
                let error_response = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from("Missing host information in request"))
                    .unwrap();
                return RequestOrResponse::Response(error_response);
            };

            // Look up the Ghost Server port for this host
            let ghost_port = routing_table
                .get(&host)
                .copied()
                .or(fallback_port)
                .unwrap_or_else(|| {
                    warn!("No Ghost Server for host: {}, using first available", host);
                    routing_table.values().next().copied().unwrap_or(0)
                });

            if ghost_port == 0 {
                error!("No Ghost Server available for host: {}", host);
                let error_response = Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(Body::from(format!("No Ghost Server for host: {}", host)))
                    .unwrap();
                return RequestOrResponse::Response(error_response);
            }

            let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

            info!(
                "Forwarding to Ghost Server: {} {} (Host: {} -> port {})",
                method, path_and_query, host, ghost_port
            );

            // Build the forwarding URI to Ghost HTTPS Server
            // Use HTTPS to trigger TLS handshake
            let ghost_uri = format!("https://127.0.0.1:{}{}", ghost_port, path_and_query)
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

            // Execute the request to Ghost HTTPS Server
            match client.request(ghost_request).await {
                Ok(ghost_response) => {
                    let status = ghost_response.status();
                    let headers = ghost_response.headers().clone();

                    info!("Ghost Server responded with status: {}", status);

                    let mut response_builder = Response::builder().status(status.as_u16());

                    for (name, value) in headers.iter() {
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
