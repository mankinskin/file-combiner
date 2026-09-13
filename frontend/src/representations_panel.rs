use dioxus::prelude::*;
use std::collections::BTreeSet;

use crate::components::RepresentationList;
use crate::models::LatentRepresentation;

/// Renders one analysis representation card, including its stage list and
/// the labelled optional-field lists (situation, goals, motivations, etc.).
#[component]
pub(crate) fn RepresentationsPanel(representations: Vec<LatentRepresentation>) -> Element {
    rsx! {
        section { class: "representation-panel", role: "region", "aria-label": "Latent representations",
            h2 { "Analysis representations" }
            for representation in representations {
                RepresentationCard { representation }
            }
        }
    }
}

#[component]
pub(crate) fn RepresentationCard(representation: LatentRepresentation) -> Element {
    rsx! {
        article { class: "representation-card",
            h3 { "{representation.title}" }
            p { "Source: {representation.source_kind}" }
            if let Some(domain) = representation.domain.as_ref().filter(|value| !value.trim().is_empty()) {
                p { "Domain: {domain}" }
            }
            if let Some(name) = representation.customer.name.as_ref().filter(|value| !value.trim().is_empty()) {
                p { "Person: {name}" }
            }
            if let Some(customer_type) = representation.customer.customer_type.as_ref().filter(|value| !value.trim().is_empty()) {
                p { "Person type: {customer_type}" }
            }
            h4 { "Stages" }
            ul { class: "representation-stages",
                for stage in &representation.stages {
                    li {
                        span { class: "stage-name", "{stage.name}" }
                        if let Some(purpose) = stage.purpose.as_ref().filter(|value| !value.trim().is_empty()) {
                            p { class: "stage-purpose", "{purpose}" }
                        }
                        if !stage.questions.is_empty() {
                            ul { class: "stage-questions",
                                for question in &stage.questions {
                                    li { "{question}" }
                                }
                            }
                        }
                        if !stage.information.is_empty() {
                            ul { class: "stage-information",
                                for information in &stage.information {
                                    li { "{information}" }
                                }
                            }
                        }
                        if !stage.target_aspects.is_empty() {
                            p { class: "stage-aspects", "Targets: {stage.target_aspects.join(\", \")}" }
                        }
                    }
                }
            }
            RepresentationList { heading: "Situation", items: representation.customer.situation.clone() }
            RepresentationList { heading: "Goals", items: representation.customer.goals.clone() }
            RepresentationList { heading: "Constraints", items: representation.customer.constraints.clone() }
            RepresentationList { heading: "Known information", items: representation.customer.known_information.clone() }
            RepresentationList { heading: "Motivations", items: representation.motivations.clone() }
            RepresentationList { heading: "Questions", items: representation.questions.clone() }
            RepresentationList { heading: "Objections", items: representation.objections.clone() }
            RepresentationList { heading: "Tone", items: representation.tone.clone() }
        }
    }
}

#[component]
pub(crate) fn CombinedRepresentationPanel(representations: Vec<LatentRepresentation>) -> Element {
    let source_titles = join_unique(
        representations
            .iter()
            .map(|representation| representation.title.as_str()),
    );
    let domains = join_unique(
        representations
            .iter()
            .filter_map(|representation| representation.domain.as_deref()),
    );
    let goals = collect_unique(
        representations
            .iter()
            .flat_map(|representation| representation.customer.goals.iter().map(String::as_str)),
    );
    let questions = collect_unique(
        representations
            .iter()
            .flat_map(|representation| representation.questions.iter().map(String::as_str)),
    );
    let tones = collect_unique(
        representations
            .iter()
            .flat_map(|representation| representation.tone.iter().map(String::as_str)),
    );

    rsx! {
        section { class: "combined-representation-panel", role: "region", "aria-label": "Combined latent representation",
            h2 { "Combined representation" }
            article { class: "representation-card combined-representation-card",
                p { class: "combined-source-line", "Sources: {source_titles}" }
                if !domains.is_empty() {
                    p { "Domains: {domains}" }
                }
                RepresentationList { heading: "Shared goals", items: goals }
                RepresentationList { heading: "Open questions", items: questions }
                RepresentationList { heading: "Conversation tone", items: tones }
            }
        }
    }
}

fn collect_unique<'a>(items: impl Iterator<Item = &'a str>) -> Vec<String> {
    items
        .filter(|item| !item.trim().is_empty())
        .map(str::trim)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn join_unique<'a>(items: impl Iterator<Item = &'a str>) -> String {
    collect_unique(items).join(", ")
}
