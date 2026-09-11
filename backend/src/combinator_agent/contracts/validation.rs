use super::types::{
    CombinedRepresentation, GeneratedArtifactContent, GeneratedArtifactKind,
    GeneratedArtifactSection, GeneratedArtifactSet, SourceFact, COMBINED_SCHEMA_VERSION,
};

impl CombinedRepresentation {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != COMBINED_SCHEMA_VERSION {
            return Err(format!(
                "unsupported combined schema version {}",
                self.schema_version
            ));
        }
        if self.title.trim().is_empty() {
            return Err("combined representation title must not be empty".to_owned());
        }
        if self.stages.len() > 32 {
            return Err("combined representation contains too many stages".to_owned());
        }
        Ok(())
    }
}

impl GeneratedArtifactSet {
    pub fn validate(&self) -> Result<(), String> {
        self.closer_overview
            .validate(GeneratedArtifactKind::CloserOverview)?;
        self.conversation_guide
            .validate(GeneratedArtifactKind::IndividualizedConversationGuide)
    }

    pub fn from_combined(combined: &CombinedRepresentation) -> Result<Self, String> {
        combined.validate()?;
        let source_facts = combined.source_facts.clone();
        let overview = GeneratedArtifactContent {
            kind: GeneratedArtifactKind::CloserOverview,
            title: format!("Closer overview for {}", combined.title),
            sections: vec![
                section(
                    "Customer situation",
                    combined.customer.situation.clone(),
                    Vec::new(),
                    source_facts.clone(),
                ),
                section(
                    "Goals and motivations",
                    combined
                        .customer
                        .goals
                        .iter()
                        .chain(&combined.motivations)
                        .cloned()
                        .collect(),
                    Vec::new(),
                    source_facts.clone(),
                ),
                section(
                    "Questions and objections",
                    combined
                        .questions
                        .iter()
                        .chain(&combined.objections)
                        .cloned()
                        .collect(),
                    Vec::new(),
                    source_facts.clone(),
                ),
            ],
            source_facts: source_facts.clone(),
            missing_input: Vec::new(),
        };
        let guide = GeneratedArtifactContent {
            kind: GeneratedArtifactKind::IndividualizedConversationGuide,
            title: format!("Individualized conversation guide for {}", combined.title),
            sections: guide_sections(combined),
            source_facts,
            missing_input: Vec::new(),
        };
        let artifacts = Self {
            closer_overview: overview,
            conversation_guide: guide,
        };
        artifacts.validate()?;
        Ok(artifacts)
    }
}

impl GeneratedArtifactContent {
    fn validate(&self, expected_kind: GeneratedArtifactKind) -> Result<(), String> {
        if self.kind != expected_kind {
            return Err(format!(
                "generated artifact kind is {:?}, expected {:?}",
                self.kind, expected_kind
            ));
        }
        if self.title.trim().is_empty() {
            return Err("generated artifact title must not be empty".to_owned());
        }
        if self.sections.is_empty() {
            return Err(format!(
                "{} must contain at least one section",
                self.kind.name()
            ));
        }
        for section in &self.sections {
            if section.heading.trim().is_empty() {
                return Err(format!(
                    "{} contains a section without a heading",
                    self.kind.name()
                ));
            }
        }
        Ok(())
    }

    pub fn to_markdown(&self) -> String {
        let mut markdown = format!("# {}\n", self.title);
        for section in &self.sections {
            markdown.push_str(&format!("\n## {}\n", section.heading));
            for item in &section.content {
                markdown.push_str(&format!("\n- {item}"));
            }
            for recommendation in &section.actionable_recommendations {
                markdown.push_str(&format!("\n\n**Recommendation:** {recommendation}"));
            }
            for fact in &section.source_facts {
                markdown.push_str(&format!(
                    "\n\n> Source fact ({}): {}",
                    fact.label, fact.excerpt
                ));
            }
            markdown.push('\n');
        }
        if !self.missing_input.is_empty() {
            markdown.push_str("\n## Missing input\n");
            for input in &self.missing_input {
                markdown.push_str(&format!("\n- {input:?}"));
            }
        }
        markdown
    }
}

fn section(
    heading: &str,
    content: Vec<String>,
    actionable_recommendations: Vec<String>,
    source_facts: Vec<SourceFact>,
) -> GeneratedArtifactSection {
    GeneratedArtifactSection {
        heading: heading.to_owned(),
        content,
        actionable_recommendations,
        source_facts,
    }
}

fn guide_sections(combined: &CombinedRepresentation) -> Vec<GeneratedArtifactSection> {
    if combined.stages.is_empty() {
        return vec![section(
            "Conversation guidance",
            vec!["No stage-specific structure was detected in the template.".to_owned()],
            combined
                .motivations
                .iter()
                .map(|motivation| format!("Explore the profile motivation: {motivation}"))
                .collect(),
            combined.source_facts.clone(),
        )];
    }
    combined
        .stages
        .iter()
        .map(|stage| {
            section(
                &stage.name,
                stage
                    .actionable_content
                    .iter()
                    .chain(&stage.questions)
                    .chain(&stage.information)
                    .cloned()
                    .collect(),
                stage.profile_grounded_recommendations.clone(),
                stage.source_facts.clone(),
            )
        })
        .collect()
}
