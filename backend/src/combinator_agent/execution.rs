//! Combine backends: the production Copilot-agent-driven backend and the
//! deterministic fake backend used by integration tests.
use std::path::Path;

use agent_builder::{
    agent::{AgentExecutor, AgentRequest, RigAgentExecutor},
    config::Config as AgentConfig,
    template::AgentTemplate,
};

use super::artifact::{
    validate_created_artifacts, CreatedArtifacts, GeneratedArtifact, WriteArtifactTool,
};
use super::template_contract::{build_prompt, parse_template_contract};
use super::{BoxFuture, CombineAgentBackend, CombineAgentError, CombineRequest};
use crate::diagnostics::{DiagnosticsRecorder, Level};

/// Caps an individual reasoning-trace message so one verbose model response
/// cannot alone exhaust a request's diagnostics retention budget.
fn truncate_for_diagnostics(text: &str) -> String {
    const MAX_CHARS: usize = 4000;
    let mut truncated: String = text.chars().take(MAX_CHARS).collect();
    if truncated.len() < text.len() {
        truncated.push_str("\u{2026} [truncated]");
    }
    truncated
}

/// Production backend: builds a fresh Copilot-backed agent per request,
/// authorizing only the request-scoped write tool on top of the template's
/// statically authorized tools.
pub struct RigCombineBackend {
    config: AgentConfig,
    template: AgentTemplate,
}

impl RigCombineBackend {
    pub fn new(config: AgentConfig, template: AgentTemplate) -> Self {
        Self { config, template }
    }
}

impl CombineAgentBackend for RigCombineBackend {
    fn combine<'a>(
        &'a self,
        dir: &'a Path,
        request: &'a CombineRequest,
        diagnostics: &'a DiagnosticsRecorder<'a>,
        direct_pat: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Vec<GeneratedArtifact>, CombineAgentError>> {
        Box::pin(async move {
            let contract = parse_template_contract(&request.first_contents);
            let created = CreatedArtifacts::default();
            let tool = WriteArtifactTool::new(
                dir.to_path_buf(),
                contract.artifacts.clone(),
                created.clone(),
            );

            diagnostics.record(
                "combine.agent_executor_setup.attempted",
                Level::Info,
                "attempting to create a Copilot agent executor with the contract-declared artifact tool",
            );
            let mut executor =
                match RigAgentExecutor::from_template(&self.config, &self.template, |server| {
                    server.tool(tool)
                })
                .await
                {
                    Ok(executor) => {
                        diagnostics.record(
                            "combine.agent_executor_setup.succeeded",
                            Level::Info,
                            "created the Copilot agent executor",
                        );
                        executor
                    }
                    Err(error) => {
                        diagnostics.record(
                            "combine.agent_executor_setup.failed",
                            Level::Error,
                            format!("failed to create the Copilot agent executor: {error}"),
                        );
                        return Err(CombineAgentError::Execution(error.to_string()));
                    }
                };

            // If direct PAT is provided, set it for request-scoped authentication
            if let Some(pat) = direct_pat {
                executor = executor.with_direct_pat(pat.to_owned());
            }

            diagnostics.record(
                "combine.agent_execution.attempted",
                Level::Info,
                "attempting to generate the contract-declared artifacts with the Copilot agent",
            );
            let prompt = match build_prompt(request) {
                Ok(prompt) => prompt,
                Err(error) => {
                    diagnostics.record(
                        "combine.prompt_construction.failed",
                        Level::Error,
                        format!("failed to construct the Copilot agent prompt: {error}"),
                    );
                    return Err(error);
                }
            };
            let response = match executor.execute(AgentRequest::new(prompt, 10)).await {
                Ok(response) => {
                    diagnostics.record(
                        "combine.agent_execution.succeeded",
                        Level::Info,
                        "the Copilot agent completed its execution",
                    );
                    response
                }
                Err(error) => {
                    diagnostics.record(
                        "combine.agent_execution.failed",
                        Level::Error,
                        format!("the Copilot agent execution failed: {error}"),
                    );
                    return Err(CombineAgentError::Execution(error.to_string()));
                }
            };

            diagnostics.record(
                "combine.agent_reasoning",
                Level::Info,
                truncate_for_diagnostics(&response.text),
            );

            diagnostics.record(
                "combine.artifact_validation.attempted",
                Level::Info,
                "validating the contract-declared artifacts created by the Copilot agent",
            );
            match validate_created_artifacts(dir, contract.artifacts.len(), created.get()) {
                Ok(artifacts) => {
                    diagnostics.record(
                        "combine.artifact_validation.succeeded",
                        Level::Info,
                        format!("validated {} generated artifacts", artifacts.len()),
                    );
                    Ok(artifacts)
                }
                Err(error) => {
                    diagnostics.record(
                        "combine.artifact_validation.failed",
                        Level::Error,
                        format!("generated artifact validation failed: {error}"),
                    );
                    Err(error)
                }
            }
        })
    }
}

/// A deterministic backend for backend integration tests: writes the
/// combined file directly through the same write tool and validation path a
/// real model-driven run would use, with no network or credentials.
pub struct FakeCombineBackend;

impl CombineAgentBackend for FakeCombineBackend {
    fn combine<'a>(
        &'a self,
        dir: &'a Path,
        request: &'a CombineRequest,
        diagnostics: &'a DiagnosticsRecorder<'a>,
        _direct_pat: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Vec<GeneratedArtifact>, CombineAgentError>> {
        Box::pin(async move {
            let contract = parse_template_contract(&request.first_contents);
            let created = CreatedArtifacts::default();
            let tool = WriteArtifactTool::new(
                dir.to_path_buf(),
                contract.artifacts.clone(),
                created.clone(),
            );
            diagnostics.record(
                "combine.agent_execution.attempted",
                Level::Info,
                "attempting deterministic artifact generation with the fake combine backend",
            );
            for artifact in &contract.artifacts {
                let content = format!("# {}\n\n{}", artifact.name, request.second_contents);
                if let Err(error) = tool.write(&artifact.name, &content).await {
                    diagnostics.record(
                        "combine.agent_execution.failed",
                        Level::Error,
                        format!(
                            "fake artifact generation failed for '{}': {error}",
                            artifact.name
                        ),
                    );
                    return Err(CombineAgentError::InvalidOutput(error.to_string()));
                }
            }
            diagnostics.record(
                "combine.agent_execution.succeeded",
                Level::Info,
                "the fake combine backend generated every contract-declared artifact",
            );
            diagnostics.record(
                "combine.artifact_validation.attempted",
                Level::Info,
                "validating the artifacts generated by the fake combine backend",
            );
            match validate_created_artifacts(dir, contract.artifacts.len(), created.get()) {
                Ok(artifacts) => {
                    diagnostics.record(
                        "combine.artifact_validation.succeeded",
                        Level::Info,
                        format!("validated {} generated artifacts", artifacts.len()),
                    );
                    Ok(artifacts)
                }
                Err(error) => {
                    diagnostics.record(
                        "combine.artifact_validation.failed",
                        Level::Error,
                        format!("fake generated artifact validation failed: {error}"),
                    );
                    Err(error)
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{CombineAgentBackend, CombineRequest, FakeCombineBackend};
    use crate::diagnostics::DiagnosticsRecorder;
    use std::fs;

    const TEMPLATE: &str = "---\nversion: 1\nconversation:\n  context: initial discovery\n  type: advisory\n  subject: retirement planning\nrole: financial adviser\npersonalization:\n  - goals\nartifacts:\n  - name: preparation-brief\n    format: markdown\n    sections:\n      - Priorities\n---\n# Brief";

    #[tokio::test]
    async fn fake_backend_produces_every_declared_artifact() {
        let dir = std::env::temp_dir().join(format!(
            "combinator-agent-execution-test-{}-{:?}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::thread::current().id(),
        ));
        fs::create_dir_all(&dir).unwrap();
        let backend = FakeCombineBackend;
        let request = CombineRequest {
            first_name: "a.txt".to_owned(),
            first_contents: TEMPLATE.replace(
                "  - name: preparation-brief\n    format: markdown\n    sections:\n      - Priorities",
                "  - name: preparation-brief\n    format: markdown\n    sections:\n      - Priorities\n  - name: question-list\n    format: markdown\n    sections:\n      - Questions",
            ),
            second_name: "b.txt".to_owned(),
            second_contents: "world".to_owned(),
        };

        let artifacts = backend
            .combine(
                &dir,
                &request,
                &DiagnosticsRecorder::new("test-request"),
                None,
            )
            .await
            .unwrap();

        assert_eq!(artifacts.len(), 2);
        assert_eq!(artifacts[0].id, "artifact-0");
        assert_eq!(artifacts[1].id, "artifact-1");
        assert_eq!(
            fs::read_to_string(&artifacts[0].path).unwrap(),
            "# preparation-brief\n\nworld"
        );
        assert_eq!(
            fs::read_to_string(&artifacts[1].path).unwrap(),
            "# question-list\n\nworld"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
