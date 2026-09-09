use dioxus::prelude::*;

mod api;
mod browser;
mod file_slot;
mod models;
mod text;

use api::{download_artifact, fetch_analysis_diagnostics, submit_analysis};
use browser::{clear_file_input, focus_element, next_frame, trigger_download};
use file_slot::FileSlot;
use models::{
    ApiErrorBody, Credential, CredentialMode, DiagnosticsView, FailureDetail, LatentRepresentation,
    RequestState, SourceState, Stage,
};

fn main() {
    dioxus::launch(App);
}

/// Turns a raw error message plus an optional diagnostics capability into a
/// full attempted/failed trace, instead of leaving the generic server
/// message as the only thing the user ever sees.
async fn build_failure(
    credential: &Credential,
    error: ApiErrorBody,
    capability: Option<String>,
) -> (FailureDetail, Option<DiagnosticsView>) {
    let mut details = error.details.clone();
    let Some(capability) = capability else {
        return (
            FailureDetail {
                code: error.code,
                message: error.message,
                status: error.status,
                details,
                trace: Vec::new(),
            },
            None,
        );
    };
    match fetch_analysis_diagnostics(credential, &capability).await {
        Ok(view) => {
            let trace = view
                .events
                .iter()
                .map(|event| event.message.clone())
                .collect();
            (
                FailureDetail {
                    code: error.code,
                    message: error.message,
                    status: error.status,
                    details,
                    trace,
                },
                Some(view),
            )
        }
        Err(reason) => {
            details.push(format!("Diagnostics lookup failed: {reason}"));
            (
                FailureDetail {
                    code: error.code,
                    message: error.message,
                    status: error.status,
                    details,
                    trace: Vec::new(),
                },
                None,
            )
        }
    }
}

/// Combines two authenticated text sources through same-origin `/api/combine`.
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
        spawn(async move {
            state.set(RequestState::Submitting(Stage::AccessValidation));
            next_frame().await;
            state.set(RequestState::Submitting(Stage::Analyzing));
            next_frame().await;
            diagnostics.set(None);
            representations.set(Vec::new());
            let (template, template_capability) =
                submit_analysis(&credential_value, "template", &first, None).await;
            match template {
                Ok(template_response) => {
                    representations.set(template_response.representations.clone());
                    state.set(RequestState::Submitting(Stage::Combining));
                    next_frame().await;
                    let (profile, profile_capability) = submit_analysis(
                        &credential_value,
                        "profile",
                        &second,
                        Some(&template_response.session_id),
                    )
                    .await;
                    match profile {
                        Ok(response) => {
                            representations.set(response.representations);
                            if let Some(capability) = &profile_capability {
                                if let Ok(view) =
                                    fetch_analysis_diagnostics(&credential_value, capability).await
                                {
                                    diagnostics.set(Some(view));
                                }
                            }
                            if response.complete {
                                state.set(RequestState::Submitting(Stage::PreparingDownload));
                                state.set(RequestState::Success(models::ArtifactList {
                                    artifacts: response.artifacts,
                                }));
                            } else {
                                state.set(RequestState::Failed(FailureDetail {
                                    code: "analysis_incomplete".to_string(),
                                    message: "The analysis session is incomplete".to_string(),
                                    status: None,
                                    details: vec!["The backend accepted both artifacts but did not return generated outputs.".to_string()],
                                    trace: Vec::new(),
                                }));
                            }
                        }
                        Err(message) => {
                            let (failure, view) =
                                build_failure(&credential_value, message, profile_capability).await;
                            diagnostics.set(view);
                            state.set(RequestState::Failed(failure));
                        }
                    }
                }
                Err(message) => {
                    let (failure, view) =
                        build_failure(&credential_value, message, template_capability).await;
                    diagnostics.set(view);
                    state.set(RequestState::Failed(failure));
                }
            }
        });
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
                    div { class: "credential-row",
                        label { r#for: "credential-mode", "Access method" }
                        select {
                            id: "credential-mode",
                            value: match *credential_mode.read() {
                                CredentialMode::SharedKey => "shared-key",
                                CredentialMode::Group => "group",
                                CredentialMode::DirectPat => "direct-pat",
                            },
                            disabled: busy,
                            onchange: move |evt| match evt.value().as_str() {
                                "group" => credential_mode.set(CredentialMode::Group),
                                "direct-pat" => credential_mode.set(CredentialMode::DirectPat),
                                _ => credential_mode.set(CredentialMode::SharedKey),
                            },
                            option { value: "shared-key", "Shared access key" }
                            option { value: "group", "Group credentials" }
                            option { value: "direct-pat", "Copilot personal access token" }
                        }
                        match *credential_mode.read() {
                            CredentialMode::SharedKey => rsx! {
                                input {
                                    id: "token-input",
                                    r#type: "password",
                                    placeholder: "Shared access key",
                                    value: "{shared_key}",
                                    disabled: busy,
                                    "aria-label": "Access key",
                                    oninput: move |evt| shared_key.set(evt.value()),
                                }
                            },
                            CredentialMode::Group => rsx! {
                                input {
                                    id: "group-name-input",
                                    r#type: "text",
                                    placeholder: "Group name",
                                    value: "{group_name}",
                                    disabled: busy,
                                    "aria-label": "Group name",
                                    oninput: move |evt| group_name.set(evt.value()),
                                }
                                input {
                                    id: "group-password-input",
                                    r#type: "password",
                                    placeholder: "Group password",
                                    value: "{group_password}",
                                    disabled: busy,
                                    "aria-label": "Group password",
                                    oninput: move |evt| group_password.set(evt.value()),
                                }
                            },
                            CredentialMode::DirectPat => rsx! {
                                input {
                                    id: "direct-pat-input",
                                    r#type: "password",
                                    placeholder: "Copilot personal access token",
                                    value: "{direct_pat}",
                                    disabled: busy,
                                    "aria-label": "Copilot personal access token",
                                    oninput: move |evt| direct_pat.set(evt.value()),
                                }
                            },
                        }
                    }
                }
                div { class: "sources-grid",
                    FileSlot { label: "Conversation template", slot_id: "first-file", source: first_source }
                    FileSlot { label: "Person information", slot_id: "second-file", source: second_source }
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
                        div { class: "request-error", role: "alert",
                            p { class: "request-error-message", "{failure.message}" }
                            ul { class: "request-error-details",
                                li { "Code: {failure.code}" }
                                if let Some(status) = failure.status {
                                    li { "HTTP status: {status}" }
                                }
                                for detail in &failure.details {
                                    li { "{detail}" }
                                }
                            }
                            if !failure.trace.is_empty() {
                                ol { class: "request-error-trace",
                                    for line in &failure.trace {
                                        li { "{line}" }
                                    }
                                }
                            }
                        }
                    }
                }
                if let RequestState::Submitting(stage) = &*state.read() {
                    section { class: "processing-panel", role: "status", "aria-live": "polite",
                        h2 { "Processing" }
                        ul {
                            for s in Stage::ALL {
                                {
                                    let class_name: &str = if s.index() < stage.index() {
                                        "done"
                                    } else if s == *stage {
                                        "active"
                                    } else {
                                        "pending"
                                    };
                                    rsx! {
                                        li { class: class_name, "{s.label()}" }
                                    }
                                }
                            }
                        }
                    }
                }
                if !representations.read().is_empty() {
                    section { class: "representation-panel", role: "region", "aria-label": "Latent representations",
                        h2 { "Analysis representations" }
                        for representation in representations.read().iter() {
                            article { class: "representation-card",
                                h3 { "{representation.title}" }
                                p { "Source: {representation.source_kind}" }
                                if let Some(domain) = &representation.domain {
                                    p { "Domain: {domain}" }
                                }
                                h4 { "Stages" }
                                ul {
                                    for stage in &representation.stages {
                                        li { "{stage.name}" }
                                    }
                                }
                                if !representation.customer.goals.is_empty() {
                                    h4 { "Goals" }
                                    ul {
                                        for goal in &representation.customer.goals {
                                            li { "{goal}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if let RequestState::Success(result) = state.read().clone() {
                    section { class: "result-panel", role: "region", "aria-label": "Generated artifacts",
                        h2 { "Generated artifacts" }
                        ul { class: "artifact-list",
                            for artifact in result.artifacts {
                                {
                                    let credential = credential.clone();
                                    let artifact_url = artifact.download_url.clone();
                                    let extension = if artifact.format == "markdown" {
                                        "md"
                                    } else {
                                        artifact.format.as_str()
                                    };
                                    let filename = format!("{}.{}", artifact.name, extension);
                                    let label = format!("Download {}", artifact.name);
                                    rsx! {
                                        li { key: "{artifact.id}",
                                            span { "{artifact.name} ({artifact.format})" }
                                            button {
                                                class: "primary",
                                                onclick: move |_| {
                                                    let credential = credential.clone();
                                                    let artifact_url = artifact_url.clone();
                                                    let filename = filename.clone();
                                                    spawn(async move {
                                                        match download_artifact(&credential, &artifact_url).await {
                                                            Ok(bytes) => trigger_download(&filename, &bytes),
                                                            Err(message) => state.set(RequestState::Failed(FailureDetail {
                                                                code: "download_failed".to_string(),
                                                                message,
                                                                status: None,
                                                                details: vec!["The browser could not retrieve the generated artifact from the backend.".to_string()],
                                                                trace: Vec::new(),
                                                            })),
                                                        }
                                                    });
                                                },
                                                "{label}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
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
}
