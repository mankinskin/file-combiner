use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const LATENT_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisKind {
    Template,
    PersonProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LatentRepresentation {
    pub schema_version: u8,
    pub source_kind: SourceKind,
    pub title: String,
    pub domain: Option<String>,
    pub stages: Vec<ConversationStage>,
    pub customer: CustomerContext,
    pub motivations: Vec<String>,
    pub questions: Vec<String>,
    pub objections: Vec<String>,
    pub tone: Vec<String>,
    pub source_spans: Vec<SourceSpan>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Template,
    PersonProfile,
    Combined,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationStage {
    pub name: String,
    pub purpose: Option<String>,
    pub questions: Vec<String>,
    pub information: Vec<String>,
    pub target_aspects: Vec<TargetAspect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetAspect {
    Customer,
    Product,
    Situation,
    Process,
    Goals,
    Constraints,
    DecisionProcess,
    Motivation,
    Objections,
    Other(String),
}

impl TargetAspect {
    fn as_str(&self) -> &str {
        match self {
            TargetAspect::Customer => "customer",
            TargetAspect::Product => "product",
            TargetAspect::Situation => "situation",
            TargetAspect::Process => "process",
            TargetAspect::Goals => "goals",
            TargetAspect::Constraints => "constraints",
            TargetAspect::DecisionProcess => "decision_process",
            TargetAspect::Motivation => "motivation",
            TargetAspect::Objections => "objections",
            TargetAspect::Other(value) => value.as_str(),
        }
    }
}

impl Serialize for TargetAspect {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TargetAspect {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let normalized = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        Ok(match normalized.as_str() {
            "customer" => TargetAspect::Customer,
            "product" => TargetAspect::Product,
            "situation" => TargetAspect::Situation,
            "process" => TargetAspect::Process,
            "goals" | "goal" => TargetAspect::Goals,
            "constraints" | "constraint" => TargetAspect::Constraints,
            "decision_process" | "decision" => TargetAspect::DecisionProcess,
            "motivation" | "motivations" => TargetAspect::Motivation,
            "objection" | "objections" => TargetAspect::Objections,
            _ => TargetAspect::Other(value),
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerContext {
    pub name: Option<String>,
    pub situation: Vec<String>,
    pub goals: Vec<String>,
    pub constraints: Vec<String>,
    pub known_information: Vec<String>,
    pub customer_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceSpan {
    pub label: String,
    pub excerpt: String,
}

impl LatentRepresentation {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != LATENT_SCHEMA_VERSION {
            return Err(format!(
                "unsupported latent schema version {}",
                self.schema_version
            ));
        }
        if self.title.trim().is_empty() {
            return Err("latent title must not be empty".to_owned());
        }
        if self.stages.len() > 32 {
            return Err("latent representation contains too many stages".to_owned());
        }
        if self.questions.len() + self.objections.len() > 256 {
            return Err("latent representation contains too many questions".to_owned());
        }
        Ok(())
    }
}

pub fn analyze_text(kind: AnalysisKind, name: &str, contents: &str) -> LatentRepresentation {
    let lower = contents.to_lowercase();
    let domain = if lower.contains("closing")
        || lower.contains("sales")
        || lower.contains("angebot")
        || lower.contains("pitch")
    {
        Some("sales".to_owned())
    } else if lower.contains("unterricht") || lower.contains("lernen") {
        Some("teaching".to_owned())
    } else if lower.contains("bewerbung") || lower.contains("recruit") {
        Some("recruiting".to_owned())
    } else {
        None
    };
    let lines = contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let mut stages = Vec::new();
    let mut questions = Vec::new();
    let mut information = Vec::new();
    let mut motivations = Vec::new();
    let mut source_spans = Vec::new();

    for line in lines {
        let normalized = line.trim_start_matches('#').trim();
        if line.starts_with('#') {
            stages.push(ConversationStage {
                name: normalized.to_owned(),
                purpose: None,
                questions: Vec::new(),
                information: Vec::new(),
                target_aspects: vec![TargetAspect::Customer, TargetAspect::Situation],
            });
        } else if line.contains('?') {
            questions.push(normalized.to_owned());
        } else if line.starts_with('-') || line.starts_with('*') {
            information.push(normalized.trim_start_matches(['-', '*']).trim().to_owned());
        }
        if matches!(kind, AnalysisKind::PersonProfile)
            && (normalized.to_lowercase().contains("ziel")
                || normalized.to_lowercase().contains("wunsch")
                || normalized.to_lowercase().contains("goal"))
        {
            motivations.push(normalized.to_owned());
        }
        if source_spans.len() < 32 && normalized.len() <= 240 {
            source_spans.push(SourceSpan {
                label: name.to_owned(),
                excerpt: normalized.to_owned(),
            });
        }
    }

    if stages.is_empty() {
        stages.push(ConversationStage {
            name: match kind {
                AnalysisKind::Template => "Conversation structure".to_owned(),
                AnalysisKind::PersonProfile => "Person context".to_owned(),
            },
            purpose: Some("Structure inferred from uploaded content".to_owned()),
            questions: questions.clone(),
            information: information.clone(),
            target_aspects: vec![TargetAspect::Situation],
        });
    }
    stages[0].questions = questions.clone();
    stages[0].information = information.clone();

    LatentRepresentation {
        schema_version: LATENT_SCHEMA_VERSION,
        source_kind: match kind {
            AnalysisKind::Template => SourceKind::Template,
            AnalysisKind::PersonProfile => SourceKind::PersonProfile,
        },
        title: name.to_owned(),
        domain,
        stages,
        customer: CustomerContext {
            situation: information,
            goals: motivations.clone(),
            ..CustomerContext::default()
        },
        motivations,
        questions,
        objections: Vec::new(),
        tone: vec!["clear".to_owned(), "respectful".to_owned()],
        source_spans,
    }
}

pub fn combine_representations(
    template: &LatentRepresentation,
    profile: &LatentRepresentation,
) -> LatentRepresentation {
    let mut stages = template.stages.clone();
    for stage in &profile.stages {
        if let Some(existing) = stages
            .iter_mut()
            .find(|candidate| candidate.name == stage.name)
        {
            existing.questions.extend(stage.questions.clone());
            existing.information.extend(stage.information.clone());
        }
    }
    LatentRepresentation {
        schema_version: LATENT_SCHEMA_VERSION,
        source_kind: SourceKind::Combined,
        title: format!("{} for {}", template.title, profile.title),
        domain: template.domain.clone().or_else(|| profile.domain.clone()),
        stages,
        customer: profile.customer.clone(),
        motivations: profile
            .motivations
            .iter()
            .chain(template.motivations.iter())
            .cloned()
            .collect(),
        questions: template
            .questions
            .iter()
            .chain(profile.questions.iter())
            .cloned()
            .collect(),
        objections: template
            .objections
            .iter()
            .chain(profile.objections.iter())
            .cloned()
            .collect(),
        tone: template.tone.clone(),
        source_spans: template
            .source_spans
            .iter()
            .chain(profile.source_spans.iter())
            .take(64)
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_round_trips_global_schema() {
        let value = LatentRepresentation {
            schema_version: LATENT_SCHEMA_VERSION,
            source_kind: SourceKind::Template,
            title: "Closing conversation".to_owned(),
            domain: Some("sales".to_owned()),
            stages: vec![ConversationStage {
                name: "Opening".to_owned(),
                purpose: Some("Set the agenda".to_owned()),
                questions: vec!["Ready to start?".to_owned()],
                information: vec![],
                target_aspects: vec![TargetAspect::Situation],
            }],
            customer: CustomerContext::default(),
            motivations: vec![],
            questions: vec![],
            objections: vec![],
            tone: vec!["direct".to_owned()],
            source_spans: vec![],
        };

        value.validate().unwrap();
        let encoded = serde_json::to_string(&value).unwrap();
        let decoded: LatentRepresentation = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value, decoded);
    }

    #[test]
    fn target_aspects_accept_observed_and_future_model_labels() {
        let json = r#"["goals", "decision_process", "pain_points"]"#;
        let decoded: Vec<TargetAspect> = serde_json::from_str(json).unwrap();

        assert_eq!(
            decoded,
            vec![
                TargetAspect::Goals,
                TargetAspect::DecisionProcess,
                TargetAspect::Other("pain_points".to_owned())
            ]
        );
        assert_eq!(
            serde_json::to_string(&decoded).unwrap(),
            r#"["goals","decision_process","pain_points"]"#
        );
    }
}
