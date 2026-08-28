use agent_builder::config::Config as AgentConfig;
use agent_builder::template::AgentTemplate;
use backend::combinator_agent::{CombineAgentBackend, FakeCombineBackend, RigCombineBackend};
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

    // COMBINE_AGENT_BACKEND=fake is for smoke tests only: it exercises the
    // full HTTP/UI flow without requiring live Copilot credentials.
    let backend: Arc<dyn CombineAgentBackend> = if config.use_fake_combine_backend {
        Arc::new(FakeCombineBackend)
    } else {
        let assets_dir = std::path::Path::new(&config.assets_dir);
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
        Arc::new(RigCombineBackend::new(agent_config, agent_template))
    };

    let state = AppState::new(
        config.bearer_token.clone(),
        config.probe_token.clone(),
        backend,
    );
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|err| panic!("failed to bind {addr}: {err}"));

    axum::serve(listener, router(state, &config.static_dir))
        .await
        .unwrap();
}
