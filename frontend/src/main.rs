use dioxus::prelude::*;

mod api;
mod artifacts_panel;
mod browser;
mod components;
mod credential_panel;
mod diagnostics_panel;
mod error_panel;
mod file_slot;
mod markdown;
mod models;
mod progress_panel;
mod representations_panel;
mod submission;
mod text;
mod workflow_panel;

use artifacts_panel::ArtifactsPanel;
use browser::{clear_file_input, focus_element};
use credential_panel::CredentialPanel;
use diagnostics_panel::DiagnosticsPanel;
use error_panel::RequestErrorPanel;
use models::{
    Credential, CredentialMode, DiagnosticsView, LatentRepresentation, RequestState, SourceState,
    Stage, WorkbenchArtifact,
};
use progress_panel::ProgressPanel;
use submission::{run_submission, SubmissionSignals};
use workflow_panel::WorkflowPanel;

fn main() {
    dioxus::launch(App);
}

/// Combines two authenticated text sources through same-origin `/api/analyze`.
/// The token and source content live only in this component's memory; a
/// page reload clears both.
#[component]
fn App() -> Element {
    let mut credential_mode = use_signal(|| CredentialMode::SharedKey);
    let mut shared_key = use_signal(String::new);
    let mut group_name = use_signal(String::new);
    let mut group_password = use_signal(String::new);
    let mut direct_pat = use_signal(String::new);
    let mut first_source = use_signal(SourceState::default);
    let mut second_source = use_signal(SourceState::default);
    let mut state = use_signal(|| RequestState::Idle);
    let mut diagnostics = use_signal(|| Option::<DiagnosticsView>::None);
    let mut representations = use_signal(Vec::<LatentRepresentation>::new);
    let mut workbench_artifacts = use_signal(Vec::<WorkbenchArtifact>::new);
    let mut reached_stage = use_signal(|| Option::<Stage>::None);
    let mut copy_notice = use_signal(|| Option::<String>::None);

    let credential = match *credential_mode.read() {
        CredentialMode::SharedKey => Credential::SharedKey(shared_key.read().clone()),
        CredentialMode::Group => Credential::Group {
            name: group_name.read().clone(),
            password: group_password.read().clone(),
        },
        CredentialMode::DirectPat => Credential::DirectPat(direct_pat.read().clone()),
    };
    let busy = matches!(*state.read(), RequestState::Submitting(_));
    let can_submit = credential.is_complete()
        && !first_source.read().content.trim().is_empty()
        && !second_source.read().content.trim().is_empty()
        && !busy;

    let submit_credential = credential.clone();
    let on_submit = move |_| {
        let credential_value = submit_credential.clone();
        let first = first_source.read().clone();
        let second = second_source.read().clone();
        spawn(run_submission(
            credential_value,
            first,
            second,
            SubmissionSignals {
                state,
                reached_stage,
                diagnostics,
                representations,
                workbench_artifacts,
                copy_notice,
            },
        ));
    };

    let on_reset = move |_| {
        credential_mode.set(CredentialMode::SharedKey);
        shared_key.set(String::new());
        group_name.set(String::new());
        group_password.set(String::new());
        direct_pat.set(String::new());
        first_source.set(SourceState::default());
        second_source.set(SourceState::default());
        state.set(RequestState::Idle);
        diagnostics.set(None);
        representations.set(Vec::new());
        workbench_artifacts.set(Vec::new());
        reached_stage.set(None);
        copy_notice.set(None);
        clear_file_input("first-file");
        clear_file_input("second-file");
        focus_element("token-input");
    };

    rsx! {
        div { class: "app-shell",
            main { class: "workspace",
                header { class: "workspace-header",
                    h1 { "file-combiner" }
                    p { "Choose an access method, then paste or upload a conversation template and person information to generate tailored preparation artifacts." }
                    CredentialPanel {
                        credential_mode,
                        shared_key,
                        group_name,
                        group_password,
                        direct_pat,
                        busy,
                    }
                }
                WorkflowPanel {
                    first_source,
                    second_source,
                    representations: representations.read().clone(),
                }
                div { class: "actions-row",
                    button {
                        class: "primary",
                        disabled: !can_submit,
                        "aria-busy": if busy { "true" } else { "false" },
                        onclick: on_submit,
                        "Generate guide"
                    }
                    button {
                        r#type: "button",
                        class: "secondary",
                        disabled: busy,
                        onclick: on_reset,
                        "Reset"
                    }
                    if let RequestState::Failed(failure) = &*state.read() {
                        RequestErrorPanel { failure: failure.clone() }
                    }
                }
                if let Some(reached) = *reached_stage.read() {
                    ProgressPanel { reached, state }
                }
                if !workbench_artifacts.read().is_empty() {
                    ArtifactsPanel { credential: credential.clone(), workbench_artifacts, copy_notice }
                }
            }
            DiagnosticsPanel { diagnostics }
        }
    }
}
