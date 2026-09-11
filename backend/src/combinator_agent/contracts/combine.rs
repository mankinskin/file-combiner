use super::super::latent::{ConversationStage, LatentRepresentation, SourceKind};
use super::types::{
    CombinationError, CombinedRepresentation, CombinedStage, MissingInput, SourceFact,
    COMBINED_SCHEMA_VERSION,
};

pub fn combine_representations(
    template: Option<&LatentRepresentation>,
    profile: Option<&LatentRepresentation>,
) -> Result<CombinedRepresentation, CombinationError> {
    let mut missing_input = Vec::new();
    let Some(template) = template else {
        missing_input.push(MissingInput::Template);
        return Err(CombinationError::MissingInput(missing_input));
    };
    let Some(profile) = profile else {
        missing_input.push(MissingInput::PersonProfile);
        return Err(CombinationError::MissingInput(missing_input));
    };
    if template.source_kind != SourceKind::Template {
        return Err(CombinationError::InvalidInput(
            "template input has the wrong source kind".to_owned(),
        ));
    }
    if profile.source_kind != SourceKind::PersonProfile {
        return Err(CombinationError::InvalidInput(
            "person profile input has the wrong source kind".to_owned(),
        ));
    }

    let mut stages = template
        .stages
        .iter()
        .map(|stage| combined_stage(stage, profile))
        .collect::<Vec<_>>();
    for stage in &profile.stages {
        if !stages.iter().any(|candidate| candidate.name == stage.name) {
            stages.push(combined_stage(stage, profile));
        }
    }
    let source_facts = source_facts(template, SourceKind::Template)
        .into_iter()
        .chain(source_facts(profile, SourceKind::PersonProfile))
        .collect::<Vec<_>>();
    let combined = CombinedRepresentation {
        schema_version: COMBINED_SCHEMA_VERSION,
        title: format!("{} for {}", template.title, profile.title),
        domain: template.domain.clone().or_else(|| profile.domain.clone()),
        stages,
        customer: profile.customer.clone(),
        motivations: profile
            .motivations
            .iter()
            .chain(&template.motivations)
            .cloned()
            .collect(),
        questions: template
            .questions
            .iter()
            .chain(&profile.questions)
            .cloned()
            .collect(),
        objections: template
            .objections
            .iter()
            .chain(&profile.objections)
            .cloned()
            .collect(),
        tone: template.tone.clone(),
        source_facts,
    };
    combined
        .validate()
        .map_err(CombinationError::InvalidInput)?;
    Ok(combined)
}

fn combined_stage(stage: &ConversationStage, profile: &LatentRepresentation) -> CombinedStage {
    let profile_stage = profile
        .stages
        .iter()
        .find(|candidate| candidate.name == stage.name);
    let profile_recommendations = profile
        .customer
        .goals
        .iter()
        .chain(&profile.motivations)
        .map(|goal| format!("Connect this stage to the profile goal: {goal}"))
        .collect();
    CombinedStage {
        name: stage.name.clone(),
        purpose: stage.purpose.clone(),
        actionable_content: stage
            .questions
            .iter()
            .chain(&stage.information)
            .cloned()
            .chain(
                profile_stage
                    .into_iter()
                    .flat_map(|stage| stage.information.iter().cloned()),
            )
            .collect(),
        questions: stage
            .questions
            .iter()
            .cloned()
            .chain(
                profile_stage
                    .into_iter()
                    .flat_map(|stage| stage.questions.iter().cloned()),
            )
            .collect(),
        information: stage
            .information
            .iter()
            .cloned()
            .chain(
                profile_stage
                    .into_iter()
                    .flat_map(|stage| stage.information.iter().cloned()),
            )
            .collect(),
        target_aspects: stage.target_aspects.clone(),
        profile_grounded_recommendations: profile_recommendations,
        source_facts: source_facts(profile, SourceKind::PersonProfile),
    }
}

fn source_facts(representation: &LatentRepresentation, source_kind: SourceKind) -> Vec<SourceFact> {
    representation
        .source_spans
        .iter()
        .map(|span| SourceFact {
            source_kind: source_kind.clone(),
            label: span.label.clone(),
            excerpt: span.excerpt.clone(),
        })
        .collect()
}
