mod combine;
mod types;
mod validation;

pub use combine::combine_representations;
pub use types::{
    CombinationError, CombinedRepresentation, CombinedStage, GeneratedArtifactContent,
    GeneratedArtifactKind, GeneratedArtifactSection, GeneratedArtifactSet, MissingInput,
    SourceFact, COMBINED_SCHEMA_VERSION,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combinator_agent::latent::{
        AnalysisKind, ConversationStage, CustomerContext, LatentRepresentation, SourceKind,
        SourceSpan, TargetAspect,
    };

    fn representation(kind: AnalysisKind, title: &str) -> LatentRepresentation {
        LatentRepresentation {
            schema_version: crate::combinator_agent::latent::LATENT_SCHEMA_VERSION,
            source_kind: match kind {
                AnalysisKind::Template => SourceKind::Template,
                AnalysisKind::PersonProfile => SourceKind::PersonProfile,
            },
            title: title.to_owned(),
            domain: Some("sales".to_owned()),
            stages: vec![ConversationStage {
                name: "Opening".to_owned(),
                purpose: Some("Set the agenda".to_owned()),
                questions: vec!["What matters most?".to_owned()],
                information: vec!["Explain the next step".to_owned()],
                target_aspects: vec![TargetAspect::Goals],
            }],
            customer: CustomerContext {
                goals: vec!["Reach a clear decision".to_owned()],
                ..CustomerContext::default()
            },
            motivations: vec!["Reduce uncertainty".to_owned()],
            questions: vec!["What matters most?".to_owned()],
            objections: Vec::new(),
            tone: vec!["clear".to_owned()],
            source_spans: vec![SourceSpan {
                label: title.to_owned(),
                excerpt: "A cited fact".to_owned(),
            }],
        }
    }

    #[test]
    fn combination_requires_both_inputs() {
        let error = combine_representations(
            None,
            Some(&representation(AnalysisKind::PersonProfile, "Profile")),
        )
        .expect_err("missing template must not be silently combined");
        assert_eq!(
            error,
            CombinationError::MissingInput(vec![MissingInput::Template])
        );
    }

    #[test]
    fn output_generation_contract_has_required_typed_sections_and_sources() {
        let combined = combine_representations(
            Some(&representation(AnalysisKind::Template, "Template")),
            Some(&representation(AnalysisKind::PersonProfile, "Profile")),
        )
        .unwrap();
        let artifacts = GeneratedArtifactSet::from_combined(&combined).unwrap();

        artifacts.validate().unwrap();
        assert_eq!(
            artifacts.closer_overview.kind,
            GeneratedArtifactKind::CloserOverview
        );
        assert_eq!(
            artifacts.conversation_guide.kind,
            GeneratedArtifactKind::IndividualizedConversationGuide
        );
        assert!(!artifacts.conversation_guide.source_facts.is_empty());
        assert!(artifacts
            .conversation_guide
            .to_markdown()
            .contains("Recommendation"));
    }
}
