use dioxus::prelude::*;

use crate::models::DiagnosticsView;

/// Renders the diagnostics sidebar: the truncation notice, the event list,
/// or an empty-state message when no request has completed yet.
#[component]
pub(crate) fn DiagnosticsPanel(diagnostics: Signal<Option<DiagnosticsView>>) -> Element {
    rsx! {
        aside { class: "diagnostics-sidebar", role: "region", "aria-label": "Diagnostics",
            h2 { "Diagnostics" }
            if let Some(view) = diagnostics.read().clone() {
                if view.truncated {
                    p { class: "diagnostics-truncated", "Some records were dropped to stay within retention limits." }
                }
                ul {
                    for event in view.events {
                        li { class: "diagnostics-event level-{event.level}",
                            span { class: "diagnostics-kind", "{event.kind}" }
                            span { class: "diagnostics-message", "{event.message}" }
                        }
                    }
                }
            } else {
                p { class: "diagnostics-empty", "No diagnostics yet. They appear here once a request completes." }
            }
        }
    }
}
