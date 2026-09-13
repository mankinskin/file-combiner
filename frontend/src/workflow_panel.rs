use dioxus::prelude::*;

use crate::file_slot::FileSlot;
use crate::models::{LatentRepresentation, SourceState};
use crate::representations_panel::{CombinedRepresentationPanel, RepresentationCard};

#[component]
pub(crate) fn WorkflowPanel(
    first_source: Signal<SourceState>,
    second_source: Signal<SourceState>,
    representations: Vec<LatentRepresentation>,
) -> Element {
    let template_representation = representation_for_source(&representations, "template");
    let profile_representation = representation_for_source(&representations, "person_profile")
        .or_else(|| representation_for_source(&representations, "profile"));

    rsx! {
        section { class: "workflow-panel", role: "region", "aria-label": "Workflow inputs and representations",
            h2 { "Workflow" }
            div { class: "workflow-source-grid",
                article { class: "workflow-source-card",
                    div { class: "workflow-source-input",
                        FileSlot { label: "Conversation template", slot_id: "first-file", source: first_source }
                    }
                    div { class: "workflow-source-representation",
                        h3 { "Template analysis" }
                        if let Some(representation) = template_representation {
                            RepresentationCard { representation }
                        } else {
                            p { class: "workflow-placeholder", "Run the workflow to see the latent representation for this source." }
                        }
                    }
                }
                article { class: "workflow-source-card",
                    div { class: "workflow-source-input",
                        FileSlot { label: "Person information", slot_id: "second-file", source: second_source }
                    }
                    div { class: "workflow-source-representation",
                        h3 { "Person analysis" }
                        if let Some(representation) = profile_representation {
                            RepresentationCard { representation }
                        } else {
                            p { class: "workflow-placeholder", "Run the workflow to see the latent representation for this source." }
                        }
                    }
                }
            }
            if !representations.is_empty() {
                CombinedRepresentationPanel { representations }
            }
        }
    }
}

fn representation_for_source(
    representations: &[LatentRepresentation],
    source_kind: &str,
) -> Option<LatentRepresentation> {
    representations
        .iter()
        .find(|representation| representation.source_kind == source_kind)
        .cloned()
}
