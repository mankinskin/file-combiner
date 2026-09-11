use std::fmt;

use serde::{Deserialize, Serialize};

use super::super::latent::{CustomerContext, SourceKind, TargetAspect};

pub const COMBINED_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CombinedRepresentation {
    pub schema_version: u8,
    pub title: String,
    pub domain: Option<String>,
    pub stages: Vec<CombinedStage>,
    pub customer: CustomerContext,
    pub motivations: Vec<String>,
    pub questions: Vec<String>,
    pub objections: Vec<String>,
    pub tone: Vec<String>,
    pub source_facts: Vec<SourceFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CombinedStage {
    pub name: String,
    pub purpose: Option<String>,
    pub actionable_content: Vec<String>,
    pub questions: Vec<String>,
    pub information: Vec<String>,
    pub target_aspects: Vec<TargetAspect>,
    pub profile_grounded_recommendations: Vec<String>,
    pub source_facts: Vec<SourceFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceFact {
    pub source_kind: SourceKind,
    pub label: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissingInput {
    Template,
    PersonProfile,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedArtifactKind {
    CloserOverview,
    IndividualizedConversationGuide,
}

impl GeneratedArtifactKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::CloserOverview => "closer-overview",
            Self::IndividualizedConversationGuide => "individualized-conversation-guide",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedArtifactSet {
    pub closer_overview: GeneratedArtifactContent,
    pub conversation_guide: GeneratedArtifactContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedArtifactContent {
    pub kind: GeneratedArtifactKind,
    pub title: String,
    pub sections: Vec<GeneratedArtifactSection>,
    pub source_facts: Vec<SourceFact>,
    pub missing_input: Vec<MissingInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedArtifactSection {
    pub heading: String,
    pub content: Vec<String>,
    pub actionable_recommendations: Vec<String>,
    pub source_facts: Vec<SourceFact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CombinationError {
    MissingInput(Vec<MissingInput>),
    InvalidInput(String),
}

impl fmt::Display for CombinationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingInput(inputs) => write!(formatter, "missing required input: {inputs:?}"),
            Self::InvalidInput(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CombinationError {}
