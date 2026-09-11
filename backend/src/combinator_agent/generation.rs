//! Model-facing generation DTOs. Provenance and artifact identity remain
//! server-owned and are attached only after the model response is parsed.
use serde::{Deserialize, Serialize};

use super::contracts::{
    CombinedRepresentation, GeneratedArtifactContent, GeneratedArtifactKind,
    GeneratedArtifactSection, GeneratedArtifactSet,
};
use super::{response::extract_json, CombineAgentError};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeneratedArtifactDraftSet {
    closer_overview: GeneratedArtifactDraft,
    conversation_guide: GeneratedArtifactDraft,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeneratedArtifactDraft {
    title: String,
    sections: Vec<GeneratedArtifactSectionDraft>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeneratedArtifactSectionDraft {
    heading: String,
    content: Vec<String>,
    actionable_recommendations: Vec<String>,
}

pub(super) fn schema_example() -> String {
    let example = GeneratedArtifactDraftSet {
        closer_overview: GeneratedArtifactDraft {
            title: "Closer overview for Example combined plan".to_owned(),
            sections: vec![GeneratedArtifactSectionDraft {
                heading: "Customer situation".to_owned(),
                content: vec!["The customer is evaluating a change with limited time.".to_owned()],
                actionable_recommendations: vec![
                    "Lead with risk reduction and outcomes.".to_owned()
                ],
            }],
        },
        conversation_guide: GeneratedArtifactDraft {
            title: "Individualized conversation guide for Example combined plan".to_owned(),
            sections: vec![GeneratedArtifactSectionDraft {
                heading: "Opening".to_owned(),
                content: vec!["Confirm goals and the decision timeline.".to_owned()],
                actionable_recommendations: vec!["Use a direct but respectful opener.".to_owned()],
            }],
        },
    };
    serde_json::to_string_pretty(&example).expect("generation draft example serializes")
}

pub(super) fn parse_response(
    text: &str,
    combined: &CombinedRepresentation,
) -> Result<GeneratedArtifactSet, CombineAgentError> {
    let draft: GeneratedArtifactDraftSet =
        serde_json::from_str(extract_json(text)).map_err(|error| {
            CombineAgentError::Execution(format!("model returned invalid generation JSON: {error}"))
        })?;
    let artifacts = GeneratedArtifactSet {
        closer_overview: attach(
            draft.closer_overview,
            GeneratedArtifactKind::CloserOverview,
            combined,
        ),
        conversation_guide: attach(
            draft.conversation_guide,
            GeneratedArtifactKind::IndividualizedConversationGuide,
            combined,
        ),
    };
    artifacts.validate().map_err(CombineAgentError::Execution)?;
    Ok(artifacts)
}

fn attach(
    draft: GeneratedArtifactDraft,
    kind: GeneratedArtifactKind,
    combined: &CombinedRepresentation,
) -> GeneratedArtifactContent {
    let source_facts = combined.source_facts.clone();
    GeneratedArtifactContent {
        kind,
        title: draft.title,
        sections: draft
            .sections
            .into_iter()
            .map(|section| GeneratedArtifactSection {
                heading: section.heading,
                content: section.content,
                actionable_recommendations: section.actionable_recommendations,
                source_facts: source_facts.clone(),
            })
            .collect(),
        source_facts,
        missing_input: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_response, schema_example};
    use crate::combinator_agent::contracts::CombinedRepresentation;

    fn combined() -> CombinedRepresentation {
        serde_json::from_str(
            r#"{"schema_version":1,"title":"Example","domain":null,"stages":[],"customer":{"name":null,"situation":[],"goals":[],"constraints":[],"known_information":[],"customer_type":null},"motivations":[],"questions":[],"objections":[],"tone":[],"source_facts":[{"source_kind":"template","label":"template.md","excerpt":"Use a clear opening."}]}"#,
        )
        .expect("combined fixture is valid")
    }

    #[test]
    fn attaches_provenance_from_the_combined_input() {
        let artifacts = parse_response(&schema_example(), &combined()).expect("draft is valid");

        assert_eq!(artifacts.closer_overview.source_facts.len(), 1);
        assert_eq!(artifacts.closer_overview.sections[0].source_facts.len(), 1);
    }

    #[test]
    fn rejects_model_owned_provenance_fields() {
        let response = schema_example().replacen(
            "\"sections\": [",
            "\"source_facts\": [],\n    \"sections\": [",
            1,
        );
        let error = parse_response(&response, &combined())
            .expect_err("the model must not provide provenance metadata");

        assert!(error.to_string().contains("unknown field `source_facts`"));
    }
}
