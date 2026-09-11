use dioxus::prelude::*;

use crate::models::{RequestState, Stage};

/// Renders the stage checklist, marking the reached stage as active, done,
/// or failed depending on the current request state.
#[component]
pub(crate) fn ProgressPanel(reached: Stage, state: ReadOnlySignal<RequestState>) -> Element {
    let busy_now = matches!(*state.read(), RequestState::Submitting(_));
    let failed_now = matches!(*state.read(), RequestState::Failed(_));
    rsx! {
        section { class: "processing-panel", role: "status", "aria-live": "polite",
            h2 { "Progress" }
            ul {
                for s in Stage::ALL {
                    {
                        let class_name: &str = if s.index() < reached.index() {
                            "done"
                        } else if s == reached && failed_now {
                            "failed"
                        } else if s == reached && busy_now {
                            "active"
                        } else if s == reached {
                            "done"
                        } else {
                            "pending"
                        };
                        let status: &str = match class_name {
                            "done" => "completed",
                            "failed" => "failed",
                            "active" => "in progress",
                            _ => "pending",
                        };
                        rsx! {
                            li { class: class_name, "{s.label()} — {status}" }
                        }
                    }
                }
            }
        }
    }
}
