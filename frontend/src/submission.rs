use dioxus::prelude::*;

use crate::api::{download_artifact, fetch_analysis_diagnostics, submit_analysis};
use crate::browser::next_frame;
use crate::models::{
    ApiErrorBody, ArtifactList, Credential, DiagnosticsView, FailureDetail, LatentRepresentation,
    RequestState, SourceState, Stage, WorkbenchArtifact,
};

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

/// Signals the submission flow updates as it progresses. Grouped so the
/// `run` call site stays a single argument instead of eight positional ones.
pub(crate) struct SubmissionSignals {
    pub(crate) state: Signal<RequestState>,
    pub(crate) reached_stage: Signal<Option<Stage>>,
    pub(crate) diagnostics: Signal<Option<DiagnosticsView>>,
    pub(crate) representations: Signal<Vec<LatentRepresentation>>,
    pub(crate) workbench_artifacts: Signal<Vec<WorkbenchArtifact>>,
    pub(crate) copy_notice: Signal<Option<String>>,
}

/// Drives the full template/profile analysis and artifact-generation flow,
/// updating the shared signals as each stage completes.
pub(crate) async fn run_submission(
    credential: Credential,
    first: SourceState,
    second: SourceState,
    mut signals: SubmissionSignals,
) {
    signals
        .state
        .set(RequestState::Submitting(Stage::AccessValidation));
    signals.reached_stage.set(Some(Stage::AccessValidation));
    next_frame().await;
    signals
        .state
        .set(RequestState::Submitting(Stage::AnalyzingTemplate));
    signals.reached_stage.set(Some(Stage::AnalyzingTemplate));
    next_frame().await;
    signals.diagnostics.set(None);
    signals.representations.set(Vec::new());
    signals.workbench_artifacts.set(Vec::new());
    signals.copy_notice.set(None);

    let (template, template_capability) =
        submit_analysis(&credential, "template", &first, None).await;
    let template_response = match template {
        Ok(response) => response,
        Err(message) => {
            let (failure, view) = build_failure(&credential, message, template_capability).await;
            signals.diagnostics.set(view);
            signals.state.set(RequestState::Failed(failure));
            return;
        }
    };

    signals
        .representations
        .set(template_response.representations.clone());
    signals
        .state
        .set(RequestState::Submitting(Stage::AnalyzingProfile));
    signals.reached_stage.set(Some(Stage::AnalyzingProfile));
    next_frame().await;

    let (profile, profile_capability) = submit_analysis(
        &credential,
        "profile",
        &second,
        Some(&template_response.session_id),
    )
    .await;
    let response = match profile {
        Ok(response) => response,
        Err(message) => {
            let (failure, view) = build_failure(&credential, message, profile_capability).await;
            signals.diagnostics.set(view);
            signals.state.set(RequestState::Failed(failure));
            return;
        }
    };

    signals.representations.set(response.representations);
    signals
        .state
        .set(RequestState::Submitting(Stage::Combining));
    signals.reached_stage.set(Some(Stage::Combining));
    next_frame().await;
    if let Some(capability) = &profile_capability {
        if let Ok(view) = fetch_analysis_diagnostics(&credential, capability).await {
            signals.diagnostics.set(Some(view));
        }
    }

    if !response.complete {
        signals.state.set(RequestState::Failed(FailureDetail {
            code: "analysis_incomplete".to_string(),
            message: "The analysis session is incomplete".to_string(),
            status: None,
            details: vec![
                "The backend accepted both artifacts but did not return generated outputs."
                    .to_string(),
            ],
            trace: Vec::new(),
        }));
        return;
    }

    signals
        .state
        .set(RequestState::Submitting(Stage::Generating));
    signals.reached_stage.set(Some(Stage::Generating));
    signals.workbench_artifacts.set(
        response
            .artifacts
            .iter()
            .cloned()
            .map(WorkbenchArtifact::pending)
            .collect(),
    );
    next_frame().await;
    signals.state.set(RequestState::Success(ArtifactList {
        artifacts: response.artifacts.clone(),
    }));

    for (index, artifact) in response.artifacts.iter().enumerate() {
        let loaded = download_artifact(&credential, &artifact.download_url)
            .await
            .and_then(|bytes| {
                String::from_utf8(bytes)
                    .map_err(|_| "The artifact is not valid UTF-8 text.".to_string())
            });
        signals.workbench_artifacts.with_mut(|artifacts| {
            if let Some(slot) = artifacts.get_mut(index) {
                slot.content = Some(loaded);
            }
        });
    }
}
