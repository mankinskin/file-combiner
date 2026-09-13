use dioxus::prelude::*;

use crate::markdown::MarkdownView;
use crate::models::DiagnosticEventView;
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
                        DiagnosticsEventRow { event }
                    }
                }
            } else {
                p { class: "diagnostics-empty", "No diagnostics yet. They appear here once a request completes." }
            }
        }
    }
}

#[component]
fn DiagnosticsEventRow(event: DiagnosticEventView) -> Element {
    let label = event_label(&event.kind);
    let stage = event_stage(&event.kind);
    let status = event_status(&event.kind);
    let timestamp = if event.timestamp_ms == 0 {
        "live".to_owned()
    } else {
        format!("{} ms", event.timestamp_ms)
    };

    rsx! {
        li { class: "diagnostics-event level-{event.level}",
            div { class: "diagnostics-event-header",
                span { class: "diagnostics-status status-{status}", "{status}" }
                span { class: "diagnostics-kind", "{label}" }
                span { class: "diagnostics-time", "{timestamp}" }
            }
            span { class: "diagnostics-stage", "{stage}" }
            MarkdownView { content: event.message, label: format!("{label} diagnostic message") }
        }
    }
}

fn event_label(kind: &str) -> &'static str {
    match kind {
        "combine.latent_analysis.attempted" => "Latent analysis started",
        "combine.latent_analysis.succeeded" => "Latent analysis completed",
        "combine.combination_execution.attempted" => "Combination started",
        "combine.combination_execution.succeeded" => "Combination completed",
        "combine.generation_execution.attempted" => "Generation started",
        "combine.generation_execution.succeeded" => "Generation completed",
        "ui.workflow.access_validation.completed" => "Access validated",
        "ui.workflow.profile_analysis.started" => "Profile analysis started",
        "ui.workflow.combination.started" => "Combining representations",
        "ui.workflow.generation.started" => "Loading previews",
        "ui.workflow.generation.completed" => "Previews ready",
        _ => "Diagnostic event",
    }
}

fn event_stage(kind: &str) -> &'static str {
    if kind.contains("latent_analysis") || kind.contains("profile_analysis") {
        "Analysis"
    } else if kind.contains("combination") {
        "Combination"
    } else if kind.contains("generation") {
        "Generation"
    } else if kind.contains("access_validation") {
        "Access"
    } else {
        "Trace"
    }
}

fn event_status(kind: &str) -> &'static str {
    if kind.ends_with(".succeeded") || kind.ends_with(".completed") {
        "done"
    } else if kind.ends_with(".failed") {
        "failed"
    } else {
        "active"
    }
}
