//! Prompt construction and response parsing shared by every combine backend:
//! the server-owned prompt text sent to the model, and the strict JSON
//! parsing/validation of whatever the model replies with.
use agent_builder::{
    agent::RigAgentExecutor, config::Config as AgentConfig, template::AgentTemplate,
};
use rig::tool::server::ToolServer;

use crate::combinator_agent::contracts::{CombinedRepresentation, SourceFact};
use crate::combinator_agent::generation;
use crate::combinator_agent::latent::{
    self, AnalysisKind, LatentRepresentation, SourceKind, TargetAspect,
};
use crate::combinator_agent::response::extract_json;
use crate::combinator_agent::CombineAgentError;

/// Caps an individual reasoning-trace message so one verbose model response
/// cannot alone exhaust a request's diagnostics retention budget.
pub(crate) fn truncate_for_diagnostics(text: &str) -> String {
    const MAX_CHARS: usize = 4000;
    let mut truncated: String = text.chars().take(MAX_CHARS).collect();
    if truncated.len() < text.len() {
        truncated.push_str("\u{2026} [truncated]");
    }
    truncated
}

pub(crate) fn parse_latent_response(text: &str) -> Result<LatentRepresentation, CombineAgentError> {
    // Real model replies sometimes wrap the object in leading/trailing prose
    // despite instructions not to; fall back to the outermost brace pair.
    let json = extract_json(text);
    let representation: LatentRepresentation = serde_json::from_str(json).map_err(|error| {
        CombineAgentError::Execution(format!(
            "model returned invalid latent JSON: {error}; raw model response was not retained"
        ))
    })?;
    representation
        .validate()
        .map_err(CombineAgentError::Execution)?;
    Ok(representation)
}

pub(crate) fn parse_combined_response(
    text: &str,
) -> Result<CombinedRepresentation, CombineAgentError> {
    let json = extract_json(text);
    let representation: CombinedRepresentation = serde_json::from_str(json).map_err(|error| {
        CombineAgentError::Execution(format!(
            "model returned invalid combined JSON: {error}; raw model response was not retained"
        ))
    })?;
    representation
        .validate()
        .map_err(CombineAgentError::Execution)?;
    Ok(representation)
}

/// A minimal example instance, serialized to JSON, so the model is told the
/// exact field names and enum spellings `LatentRepresentation` requires
/// instead of being asked to satisfy an unnamed "schema".
pub(crate) fn latent_schema_example(kind: AnalysisKind) -> String {
    let example = LatentRepresentation {
        schema_version: latent::LATENT_SCHEMA_VERSION,
        source_kind: match kind {
            AnalysisKind::Template => latent::SourceKind::Template,
            AnalysisKind::PersonProfile => latent::SourceKind::PersonProfile,
        },
        title: match kind {
            AnalysisKind::Template => "Example conversation template".to_owned(),
            AnalysisKind::PersonProfile => "Example person profile".to_owned(),
        },
        domain: Some("sales".to_owned()),
        stages: vec![latent::ConversationStage {
            name: "Opening".to_owned(),
            purpose: Some("Set the agenda".to_owned()),
            questions: vec!["Are you ready to start?".to_owned()],
            information: vec!["Explains how the call will proceed".to_owned()],
            target_aspects: vec![latent::TargetAspect::Situation],
        }],
        customer: latent::CustomerContext {
            name: Some("Example person".to_owned()),
            situation: vec!["Runs a small business".to_owned()],
            goals: vec!["Grow monthly cash flow".to_owned()],
            constraints: vec!["Limited time".to_owned()],
            known_information: vec!["Already has some experience".to_owned()],
            customer_type: Some("owner-operator".to_owned()),
        },
        motivations: vec!["Financial independence".to_owned()],
        questions: vec!["What is the first concrete step?".to_owned()],
        objections: vec!["Worried about the time commitment".to_owned()],
        tone: vec!["direct".to_owned(), "respectful".to_owned()],
        source_spans: vec![latent::SourceSpan {
            label: "example.md".to_owned(),
            excerpt: "Are you ready to start?".to_owned(),
        }],
    };
    serde_json::to_string_pretty(&example).expect("example latent representation serializes")
}

fn combined_schema_example() -> String {
    let example = CombinedRepresentation {
        schema_version: 1,
        title: "Example combined plan".to_owned(),
        domain: Some("sales".to_owned()),
        stages: vec![crate::combinator_agent::contracts::CombinedStage {
            name: "Opening".to_owned(),
            purpose: Some("Set a productive agenda".to_owned()),
            actionable_content: vec!["Open by confirming goals".to_owned()],
            questions: vec!["What matters most to you right now?".to_owned()],
            information: vec!["Explain the next decision".to_owned()],
            target_aspects: vec![TargetAspect::Goals],
            profile_grounded_recommendations: vec![
                "Connect the first question to their goals".to_owned()
            ],
            source_facts: vec![SourceFact {
                source_kind: SourceKind::Template,
                label: "template.md".to_owned(),
                excerpt: "Focus on the customer's goals.".to_owned(),
            }],
        }],
        customer: latent::CustomerContext {
            name: Some("Example customer".to_owned()),
            situation: vec!["Evaluating a purchase".to_owned()],
            goals: vec!["Reduce risk".to_owned()],
            constraints: vec!["Limited decision time".to_owned()],
            known_information: vec!["Already familiar with the topic".to_owned()],
            customer_type: Some("decision-maker".to_owned()),
        },
        motivations: vec!["Reduce uncertainty".to_owned()],
        questions: vec!["What is the key trade-off?".to_owned()],
        objections: vec!["The change feels expensive".to_owned()],
        tone: vec!["clear".to_owned(), "direct".to_owned()],
        source_facts: vec![SourceFact {
            source_kind: SourceKind::PersonProfile,
            label: "profile.md".to_owned(),
            excerpt: "The customer wants fewer surprises.".to_owned(),
        }],
    };
    serde_json::to_string_pretty(&example).expect("combined representation serializes")
}

pub(crate) fn build_combination_prompt(
    template_name: &str,
    template_contents: &str,
    profile_name: &str,
    profile_contents: &str,
) -> String {
    let schema_example = combined_schema_example();
    format!(
        "You are a server-owned combination stage. Combine the two analyzed inputs into exactly one JSON object matching the Typed CombinedRepresentation contract.\n\nserver-owned instruction:\nCreate a CombinedRepresentation from the template and person profile. Copy only supported facts from the supplied sources; never invent unstated details. Keep template and profile evidence distinct and carry the relevant source facts into the final combined representation.\n\nTemplate source ({template_name}):\n--- BEGIN FILE ---\n{template_contents}\n--- END FILE ---\n\nProfile source ({profile_name}):\n--- BEGIN FILE ---\n{profile_contents}\n--- END FILE ---\n\nReply with exactly one raw JSON object and nothing else. Use the schema below as the contract:\n{schema_example}\n\nDo not wrap the JSON in markdown fences or prose."
    )
}

pub(crate) fn build_generation_prompt(
    template_name: &str,
    profile_name: &str,
    combined_json: &str,
) -> String {
    let schema_example = generation::schema_example();
    format!(
        "You are a server-owned generation stage. Produce a content draft from the combined representation. The draft must include both a closer overview and an individualized conversation guide, each with a title and non-empty sections.\n\nserver-owned instruction:\nGenerate only reader-facing title, section heading, content, and actionable_recommendations values from the supplied CombinedRepresentation JSON. Do not return artifact kinds, source facts, missing-input fields, IDs, or filenames: the server owns and attaches all metadata.\n\nTemplate source ({template_name})\nProfile source ({profile_name})\nCombined representation JSON:\n{combined_json}\n\nReturn exactly one raw JSON object matching this schema:\n{schema_example}\n\nDo not wrap the JSON in markdown fences or prose."
    )
}

/// Builds a tool-free executor for the latent-analysis prompt. This
/// deliberately does not reuse `RigAgentExecutor::from_template`'s preamble:
/// that preamble is authored for the tool-calling `combine()` flow ("create
/// each artifact only through a tool" / "reply briefly, do not repeat
/// contents"), which directly contradicts analysis's requirement to reply
/// with the full JSON body and no tool calls.
pub(crate) fn build_analysis_executor(
    config: &AgentConfig,
    template: &AgentTemplate,
) -> RigAgentExecutor {
    let model = config
        .model
        .clone()
        .unwrap_or_else(|| template.model.clone());
    let tool_server_handle = ToolServer::new().run();
    RigAgentExecutor::new(
        model,
        "You are a structured-data extraction assistant. You never call a tool \
         and never explain your reasoning. You respond with exactly one raw \
         JSON object satisfying the shape given in the request, and nothing \
         else.",
        tool_server_handle,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_combination_prompt, build_generation_prompt, latent_schema_example,
        parse_latent_response,
    };
    use crate::combinator_agent::generation;
    use crate::combinator_agent::latent::{AnalysisKind, SourceKind};
    use crate::combinator_agent::{
        CombineAgentError, CombinedRepresentation, LatentRepresentation,
    };

    #[test]
    fn parse_latent_response_tolerates_leading_and_trailing_prose() {
        let wrapped = format!(
            "Here is the analysis:\n```json\n{}\n```\nLet me know if you need changes.",
            latent_schema_example(AnalysisKind::Template)
        );

        let representation =
            parse_latent_response(&wrapped).expect("prose-wrapped JSON still parses");

        assert_eq!(representation.title, "Example conversation template");
    }

    #[test]
    fn latent_schema_example_uses_requested_source_kind() {
        let profile: LatentRepresentation =
            serde_json::from_str(&latent_schema_example(AnalysisKind::PersonProfile)).unwrap();

        assert_eq!(profile.source_kind, SourceKind::PersonProfile);
    }

    #[test]
    fn parse_latent_response_rejects_non_json_prose() {
        let raw_response = "I was unable to produce a structured result.";
        let error = parse_latent_response(raw_response)
            .expect_err("plain prose is not a valid latent representation");
        assert!(matches!(error, CombineAgentError::Execution(_)));
        assert!(!error.to_string().contains(raw_response));
        assert!(error
            .to_string()
            .contains("raw model response was not retained"));
    }

    #[test]
    fn generation_prompt_keeps_provenance_out_of_the_model_contract() {
        let prompt = build_generation_prompt("template.md", "profile.md", "{\"title\":\"Demo\"}");

        assert!(prompt.contains("the server owns and attaches all metadata"));
        assert!(prompt.contains("Do not return artifact kinds, source facts"));
        assert!(!prompt.contains("source_kind MUST be exactly one of"));
    }

    #[test]
    fn generation_schema_example_excludes_source_fact_provenance() {
        let schema = generation::schema_example();

        assert!(!schema.contains("source_kind"));
        assert!(!schema.contains("source_facts"));
        assert!(!schema.contains("missing_input"));
    }

    #[test]
    fn generation_prompt_names_both_required_artifacts() {
        let prompt = build_generation_prompt("template.md", "profile.md", "{\"title\":\"Demo\"}");

        assert!(prompt.contains("closer_overview"));
        assert!(prompt.contains("conversation_guide"));
    }

    #[test]
    fn generation_schema_does_not_accept_model_owned_metadata() {
        let response = generation::schema_example().replacen(
            "\"sections\": [",
            "\"source_facts\": [],\n    \"sections\": [",
            1,
        );
        let combined: CombinedRepresentation = serde_json::from_str(
            r#"{"schema_version":1,"title":"Demo","domain":null,"stages":[],"customer":{"name":null,"situation":[],"goals":[],"constraints":[],"known_information":[],"customer_type":null},"motivations":[],"questions":[],"objections":[],"tone":[],"source_facts":[]}"#,
        )
        .unwrap();
        let error = generation::parse_response(&response, &combined)
            .expect_err("model-owned metadata must be rejected");

        assert!(error.to_string().contains("invalid generation JSON"));
        assert!(error.to_string().contains("unknown field `source_facts`"));
    }

    #[test]
    fn isolated_prompt_chain_uses_typed_combination_and_generation_contracts() {
        let combination_prompt =
            build_combination_prompt("template.md", "template body", "profile.md", "profile body");
        assert!(combination_prompt.contains("CombinedRepresentation"));
        assert!(combination_prompt.contains("server-owned instruction"));
        assert!(combination_prompt.contains("template.md"));

        let generation_prompt =
            build_generation_prompt("template.md", "profile.md", "{\"title\":\"Demo\"}");
        assert!(generation_prompt.contains("content draft"));
        assert!(generation_prompt.contains("closer_overview"));
        assert!(generation_prompt.contains("conversation_guide"));
    }
}
