use agent_builder::config::Config as AgentConfig;
use agent_builder::template::AgentTemplate;
use backend::combinator_agent::{CombineAgentBackend, FakeCombineBackend, RigCombineBackend};
use backend::config::AppConfig;
use backend::{router, AppState};
use hyper::server::conn::http1;
use hyper_util::rt::tokio::TokioIo;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[tokio::main]
async fn main() {
    init_tracing();

    let config = AppConfig::from_env().unwrap_or_else(|err| {
        tracing::error!("configuration error: {err}");
        std::process::exit(1);
    });

    // COMBINE_AGENT_BACKEND=fake is for smoke tests only: it exercises the
    // full HTTP/UI flow without requiring live Copilot credentials.
    let backend: Arc<dyn CombineAgentBackend> = if config.use_fake_combine_backend {
        Arc::new(FakeCombineBackend)
    } else {
        let assets_dir = std::path::Path::new(&config.assets_dir);
        let agent_config = AgentConfig::load(&assets_dir.join("agent-builder.toml"))
            .unwrap_or_else(|err| {
                tracing::error!("combinator-agent configuration error: {err}");
                std::process::exit(1);
            });
        let agent_template =
            AgentTemplate::load(&agent_config.template_path()).unwrap_or_else(|err| {
                tracing::error!("combinator-agent template error: {err}");
                std::process::exit(1);
            });
        Arc::new(RigCombineBackend::new(agent_config, agent_template))
    };

    let state = AppState::with_group(
        config.bearer_token.clone(),
        config.probe_token.clone(),
        config.group.clone(),
        backend,
    );
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|err| panic!("failed to bind {addr}: {err}"));

    // Validate and prepare TLS configuration if provided
    if let Some(tls_config) = &config.tls {
        let tls_acceptor = load_tls_acceptor(tls_config).unwrap_or_else(|err| {
            tracing::error!("TLS configuration error: {err}");
            std::process::exit(1);
        });

        tracing::info!("file-combiner backend listening on {addr} (HTTPS)");

        let state = Arc::new(state);
        serve_with_tls(listener, tls_acceptor, state, &config.static_dir).await;
    } else {
        tracing::info!("file-combiner backend listening on {addr} (HTTP, development mode)");
        axum::serve(listener, router(state, &config.static_dir))
            .await
            .unwrap();
    }
}

fn load_tls_acceptor(tls_config: &backend::config::TlsConfig) -> Result<TlsAcceptor, String> {
    use rustls_pemfile::{certs, private_key};
    use std::fs::File;
    use std::io::BufReader;

    // Load certificate chain
    let cert_file = File::open(&tls_config.cert_path)
        .map_err(|e| format!("failed to open certificate file: {e}"))?;
    let mut cert_reader = BufReader::new(cert_file);
    let cert_chain = certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("failed to parse certificate: {e}"))?;

    if cert_chain.is_empty() {
        return Err("certificate file contains no certificates".to_string());
    }

    // Load private key
    let key_file = File::open(&tls_config.key_path)
        .map_err(|e| format!("failed to open private key file: {e}"))?;
    let mut key_reader = BufReader::new(key_file);
    let key = private_key(&mut key_reader)
        .map_err(|e| format!("failed to parse private key: {e}"))?
        .ok_or_else(|| "private key file contains no keys".to_string())?;

    // Build TLS server config
    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .map_err(|e| format!("failed to configure TLS: {e}"))?;

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}

async fn serve_with_tls(
    listener: tokio::net::TcpListener,
    tls_acceptor: TlsAcceptor,
    state: Arc<AppState>,
    static_dir: &str,
) {
    use hyper::body::Incoming;
    use hyper::service::Service as HyperService;

    tracing::info!("HTTPS listener started");

    // Wrapper struct to adapt Tower Service to hyper's Service trait
    // Uses Arc to allow &self instead of &mut self
    struct TowerToHyperWrapper<S: Clone> {
        service: Arc<S>,
    }

    impl<S> HyperService<hyper::Request<Incoming>> for TowerToHyperWrapper<S>
    where
        S: tower::Service<hyper::Request<Incoming>> + Clone + 'static,
        S::Response: Send + 'static,
        S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + 'static,
        S::Future: Send + 'static,
    {
        type Response = S::Response;
        type Error = S::Error;
        type Future = S::Future;

        fn call(&self, req: hyper::Request<Incoming>) -> Self::Future {
            // Clone the service for this request to get &mut access
            let mut svc = (*self.service).clone();
            tower::Service::call(&mut svc, req)
        }
    }

    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok((s, p)) => (s, p),
            Err(e) => {
                tracing::error!("accept error: {e}");
                continue;
            }
        };

        let tls_acceptor = tls_acceptor.clone();
        let state = state.clone();
        let static_dir = static_dir.to_string();

        tokio::spawn(async move {
            match tls_acceptor.accept(stream).await {
                Ok(tls_stream) => {
                    // Create router with cloned state for this connection
                    let app = router((*state).clone(), &static_dir);

                    // Wrap TLS stream with TokioIo for hyper compatibility
                    let io = TokioIo::new(tls_stream);

                    // Wrap the Tower service to match hyper's Service trait
                    let hyper_service = TowerToHyperWrapper {
                        service: Arc::new(app),
                    };

                    // Create HTTP/1.1 handler and forward TLS stream to application router
                    let builder = http1::Builder::new();
                    if let Err(err) = builder.serve_connection(io, hyper_service).await {
                        tracing::debug!("HTTP/1.1 handler error from {peer_addr}: {err}");
                    }
                }
                Err(err) => {
                    tracing::debug!("TLS handshake error from {peer_addr}: {err}");
                }
            }
        });
    }
}
