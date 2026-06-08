use std::sync::Arc;
use tower::Service;
use tracing_subscriber::EnvFilter;

async fn log_request(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    tracing::info!("Incoming request: {} {}", method, uri);
    next.run(req).await
}

async fn add_alt_svc_header(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(req).await;
    // Advertise HTTP/3 support on the same port (3125) UDP
    response.headers_mut().insert(
        axum::http::header::HeaderName::from_static("alt-svc"),
        axum::http::HeaderValue::from_static("h3=\":3125\""),
    );
    response
}

fn load_tls_13_config(
    cert_path: &std::path::Path,
    key_path: &std::path::Path,
) -> Result<rustls::ServerConfig, Box<dyn std::error::Error>> {
    use std::fs::File;
    use std::io::BufReader;

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok();

    let cert_file = File::open(cert_path)?;
    let mut reader = BufReader::new(cert_file);
    let certs: Vec<rustls_pki_types::CertificateDer<'static>> =
        rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?;

    let key_file = File::open(key_path)?;
    let mut reader = BufReader::new(key_file);
    let key: rustls_pki_types::PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut reader)?
        .ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "Private key not found")
    })?;

    let mut server_config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])?
    .with_no_client_auth()
    .with_single_cert(certs, key)?;

    // Set ALPN protocols for HTTP/3, HTTP/2, and HTTP/1.1
    server_config.alpn_protocols = vec![b"h3".to_vec(), b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(server_config)
}

const PROXY_TLS_SANS: &[&str] = &[
    "api.anthropic.com",
    "daily-cloudcode-pa.googleapis.com",
    "cloudcode-pa.googleapis.com",
];

fn proxy_cert_has_required_sans(cert_path: &std::path::Path) -> bool {
    let output = std::process::Command::new("openssl")
        .args([
            "x509",
            "-in",
            cert_path.to_str().unwrap_or(""),
            "-noout",
            "-text",
        ])
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    PROXY_TLS_SANS
        .iter()
        .all(|host| text.contains(&format!("DNS:{host}")))
}

async fn run_http3_server(
    addr: std::net::SocketAddr,
    rustls_config: rustls::ServerConfig,
    app: axum::Router,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use quinn::Endpoint;

    let quinn_server_config = quinn::crypto::rustls::QuicServerConfig::try_from(rustls_config)?;
    let quinn_config = quinn::ServerConfig::with_crypto(Arc::new(quinn_server_config));
    let endpoint = Endpoint::server(quinn_config, addr)?;
    tracing::info!("CAB HTTP/3 (QUIC) Gateway listening on udp://{}", addr);

    while let Some(new_conn) = endpoint.accept().await {
        let app = app.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_h3_connection(new_conn, app).await {
                tracing::debug!("H3 connection error: {:?}", e);
            }
        });
    }

    Ok(())
}

async fn handle_h3_connection(
    incoming: quinn::Incoming,
    app: axum::Router,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = incoming.accept()?.await?;
    let mut h3_conn = h3::server::Connection::new(h3_quinn::Connection::new(connection)).await?;

    while let Some(request_resolver) = h3_conn.accept().await? {
        let app = app.clone();
        tokio::spawn(async move {
            match request_resolver.resolve_request().await {
                Ok((req, stream)) => {
                    if let Err(e) = handle_h3_request(req, stream, app).await {
                        tracing::debug!("H3 request handler error: {:?}", e);
                    }
                }
                Err(e) => {
                    tracing::debug!("H3 resolve error: {:?}", e);
                }
            }
        });
    }

    Ok(())
}

async fn handle_h3_request<S>(
    req: axum::http::Request<()>,
    mut stream: h3::server::RequestStream<S, bytes::Bytes>,
    mut app: axum::Router,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: h3::quic::RecvStream + h3::quic::SendStream<bytes::Bytes> + 'static,
{
    // 1. Read request body if present
    let mut body_bytes = Vec::new();
    while let Some(mut chunk) = stream.recv_data().await? {
        use bytes::Buf;
        while chunk.has_remaining() {
            let c = chunk.chunk();
            body_bytes.extend_from_slice(c);
            chunk.advance(c.len());
        }
    }

    // 2. Construct axum compatible request
    let (parts, _) = req.into_parts();
    let axum_req = axum::http::Request::from_parts(parts, axum::body::Body::from(body_bytes));

    // 3. Call Axum Router Service
    let response = match app.call(axum_req).await {
        Ok(resp) => resp,
        Err(infallible) => match infallible {},
    };

    // 4. Send response headers
    let (parts, body) = response.into_parts();
    let h3_response = axum::http::Response::from_parts(parts, ());
    stream.send_response(h3_response).await?;

    // 5. Send response body
    let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("Failed to serialize response body for H3: {:?}", e);
            bytes::Bytes::new()
        }
    };
    if !body_bytes.is_empty() {
        stream.send_data(body_bytes).await?;
    }

    // 6. Finish stream
    stream.finish().await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,cab_gateway=debug,cab_api=debug")),
        )
        .init();

    // Load config
    let config = cab_core::CabConfig::load();

    // Initialize in-memory store
    let store = cab_db::init_store()
        .await
        .expect("Failed to initialize store");

    // Spawn background task to sync models.dev catalog at startup
    let store_clone = store.clone();
    tokio::spawn(async move {
        tracing::info!("Startup: Synchronizing models.dev provider/model catalog...");
        match cab_api::providers::sync_models_dev_catalog(&store_clone).await {
            Ok(count) => {
                tracing::info!(
                    "Startup: models.dev catalog synchronization finished. Synced {} models.",
                    count
                );
            }
            Err(e) => {
                tracing::error!(
                    "Startup: models.dev catalog synchronization failed: {:?}",
                    e
                );
            }
        }
    });

    // Build combined router:
    //   /v1/*  → Gateway (proxy to LLM providers)
    //   /api/* → Management REST API
    let gateway_state = cab_gateway::GatewayState::new(store.clone());
    let gateway = cab_gateway::gateway_router(gateway_state);
    let api = cab_api::api_router(store.clone());

    let build_dir = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("build");

    let app = if build_dir.exists() {
        tracing::info!("Serving frontend assets from: {}", build_dir.display());
        let serve_dir = tower_http::services::ServeDir::new(&build_dir).fallback(
            tower_http::services::ServeFile::new(build_dir.join("index.html")),
        );
        gateway
            .merge(api)
            .nest_service(
                "/_app",
                tower_http::services::ServeDir::new(build_dir.join("_app")),
            )
            .fallback_service(serve_dir)
    } else {
        tracing::warn!(
            "Frontend build directory not found at {}. Run `npm run build` to build frontend.",
            build_dir.display()
        );
        gateway.merge(api)
    }
    .layer(axum::middleware::from_fn(add_alt_svc_header))
    .layer(axum::middleware::from_fn(log_request))
    .layer(tower_http::trace::TraceLayer::new_for_http());

    // Get gateway port from persisted settings (default 3125)
    let settings = cab_db::settings::get(&store).await.unwrap_or_else(|_| {
        let mut settings = cab_db::settings::default_settings();
        settings.gateway_port = config.gateway.port as i64;
        settings
    });
    let gateway_port = settings.gateway_port as u16;

    // Split TCP addresses
    let http_addr = format!("{}:{}", config.gateway.host, gateway_port);
    let http_addr_parsed: std::net::SocketAddr =
        http_addr.parse().expect("Failed to parse HTTP address");

    let https_port = 46656;
    let https_addr = format!("{}:{}", config.gateway.host, https_port);
    let https_addr_parsed: std::net::SocketAddr =
        https_addr.parse().expect("Failed to parse HTTPS address");

    // Ensure .gemini folder exists and generate certs
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .expect(
            "Could not resolve user home directory (neither HOME nor USERPROFILE env var is set)",
        );
    let gemini_dir = std::path::PathBuf::from(home).join(".gemini");
    if !gemini_dir.exists() {
        let _ = std::fs::create_dir_all(&gemini_dir);
    }
    let cert_path = gemini_dir.join("cert.pem");
    let key_path = gemini_dir.join("key.pem");

    if !cert_path.exists() || !key_path.exists() || !proxy_cert_has_required_sans(&cert_path) {
        if cert_path.exists() {
            tracing::info!("Regenerating SSL certificate to include proxy mode SAN hostnames...");
            let _ = std::fs::remove_file(&cert_path);
            let _ = std::fs::remove_file(&key_path);
        } else {
            tracing::info!("Generating dynamic self-signed SSL certificate for CAB proxy mode...");
        }
        let status = std::process::Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-keyout",
                key_path.to_str().unwrap(),
                "-out",
                cert_path.to_str().unwrap(),
                "-days",
                "3650",
                "-nodes",
                "-subj",
                "/CN=api.anthropic.com",
                "-addext",
                "subjectAltName=DNS:api.anthropic.com,DNS:daily-cloudcode-pa.googleapis.com,DNS:cloudcode-pa.googleapis.com",
            ])
            .status();
        match status {
            Ok(s) if s.success() => tracing::info!("SSL certificate generated successfully."),
            other => tracing::error!("Failed to generate SSL certificate: {:?}", other),
        }
    }

    // Load TLS configs
    let mut https_config = None;
    let mut h3_config = None;

    if cert_path.exists() && key_path.exists() {
        match load_tls_13_config(&cert_path, &key_path) {
            Ok(server_config) => {
                let arc_config = std::sync::Arc::new(server_config);
                https_config = Some(axum_server::tls_rustls::RustlsConfig::from_config(
                    arc_config.clone(),
                ));
                h3_config = Some((*arc_config).clone());
            }
            Err(e) => {
                tracing::error!("Failed to build TLS 1.3 configuration: {:?}", e);
            }
        }
    } else {
        tracing::warn!("SSL cert/key files missing. HTTPS and HTTP/3 support will be disabled.");
    }

    let https_config_loopback_443 = https_config.clone();

    let http_service = app.clone().into_make_service();
    let https_service = app.clone().into_make_service();

    // Spawn http task (TCP)
    let http_handle = tokio::spawn(async move {
        tracing::info!("CAB HTTP Gateway running at http://{}", http_addr);
        if let Err(e) = axum_server::bind(http_addr_parsed)
            .serve(http_service)
            .await
        {
            tracing::error!("HTTP server error: {:?}", e);
        }
    });

    // Spawn https task (TCP, TLS 1.3)
    let https_handle = tokio::spawn(async move {
        if let Some(tls_cfg) = https_config {
            tracing::info!(
                "CAB HTTPS Gateway running at https://{} (TLS 1.3 Only Mode)",
                https_addr
            );
            if let Err(e) = axum_server::bind_rustls(https_addr_parsed, tls_cfg)
                .serve(https_service)
                .await
            {
                tracing::error!("HTTPS server error: {:?}", e);
            }
        } else {
            tracing::warn!("HTTPS server not started because SSL certificate failed to load");
        }
    });

    // Optional: bind 127.0.0.1:443 for Go/agy (hardcodes HTTPS port after DNS hijack).
    let https_service_443 = app.clone().into_make_service();
    let loopback_443: std::net::SocketAddr = "127.0.0.1:443".parse().expect("parse 443");
    let _loopback_443_handle = tokio::spawn(async move {
        if let Some(tls_cfg) = https_config_loopback_443 {
            tracing::info!("CAB proxy HTTPS binding https://127.0.0.1:443 (agy / Go clients)");
            if let Err(e) = axum_server::bind_rustls(loopback_443, tls_cfg)
                .serve(https_service_443)
                .await
            {
                tracing::warn!(
                    "Proxy loopback https://127.0.0.1:443 not available ({e}). \
                     Run once: sudo setcap cap_net_bind_service=+ep $(readlink -f target/debug/cab-server) && restart CAB"
                );
            }
        }
    });

    // Spawn http/3 task (UDP)
    let h3_handle = tokio::spawn(async move {
        if let Some(h3_cfg) = h3_config {
            let res = run_http3_server(http_addr_parsed, h3_cfg, app).await;
            if let Err(e) = res {
                tracing::error!("HTTP/3 server error: {:?}", e);
            }
        }
    });

    // Wait for all servers
    let _ = tokio::join!(http_handle, https_handle, h3_handle);

    Ok(())
}
