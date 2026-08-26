use backend::config::AppConfig;
use backend::{router, AppState};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let config = AppConfig::from_env().unwrap_or_else(|err| {
        eprintln!("configuration error: {err}");
        std::process::exit(1);
    });

    let state = AppState::new(config.bearer_token.clone(), config.probe_token.clone());
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|err| panic!("failed to bind {addr}: {err}"));

    axum::serve(listener, router(state)).await.unwrap();
}
