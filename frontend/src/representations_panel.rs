use dioxus::prelude::*;

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
    }
}
