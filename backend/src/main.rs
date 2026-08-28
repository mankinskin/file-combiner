use agent_builder::config::Config as AgentConfig;
use agent_builder::template::AgentTemplate;
use backend::combinator_agent::RigCombineBackend;
use backend::config::AppConfig;
use backend::{router, AppState};
use std::net::SocketAddr;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let config = AppConfig::from_env().unwrap_or_else(|err| {
        eprintln!("configuration error: {err}");
        std::process::exit(1);
    });

    let assets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let agent_config = AgentConfig::load(&assets_dir.join("agent-builder.toml"))
        .unwrap_or_else(|err| {
            eprintln!("combinator-agent configuration error: {err}");
            std::process::exit(1);
        });
    let agent_template =
        AgentTemplate::load(&agent_config.template_path()).unwrap_or_else(|err| {
            eprintln!("combinator-agent template error: {err}");
            std::process::exit(1);
        });
    let backend: Arc<dyn backend::combinator_agent::CombineAgentBackend> =
        Arc::new(RigCombineBackend::new(agent_config, agent_template));

    let state = AppState::new(config.bearer_token.clone(), config.probe_token.clone(), backend);
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|err| panic!("failed to bind {addr}: {err}"));

    axum::serve(listener, router(state, &config.static_dir))
        .await
        .unwrap();
}

