use dioxus::prelude::*;

use crate::api::download_artifact;
use crate::browser::{copy_to_clipboard, trigger_download};
use crate::markdown::MarkdownView;
use crate::models::{Credential, WorkbenchArtifact};

/// Renders the generated-artifact blocks: the loaded/loading/error content
/// view plus the copy and download actions for each artifact.
#[component]
pub(crate) fn ArtifactsPanel(
    credential: Credential,
    workbench_artifacts: Signal<Vec<WorkbenchArtifact>>,
    copy_notice: Signal<Option<String>>,
) -> Element {
    let mut workbench_artifacts = workbench_artifacts;
    let mut copy_notice = copy_notice;

    rsx! {
        section { class: "result-panel", role: "region", "aria-label": "Generated artifacts",
            h2 { "Generated artifacts" }
            if let Some(notice) = copy_notice.read().clone() {
                p { class: "copy-notice", role: "status", "aria-live": "polite", "{notice}" }
            }
            div { class: "artifact-blocks",
                for (index, entry) in workbench_artifacts.read().iter().enumerate() {
                    {
                        let credential = credential.clone();
                        let artifact_url = entry.artifact.download_url.clone();
                        let filename = entry.file_name();
                        let name = entry.artifact.name.clone();
                        let format = entry.artifact.format.clone();
                        let download_label = format!("Download {name}");
                        let copy_label = format!("Copy {name}");
                        let content = entry.content.clone();
                        let copy_text = match &content {
                            Some(Ok(text)) => Some(text.clone()),
                            _ => None,
                        };
                        rsx! {
                            article { key: "{entry.artifact.id}", class: "artifact-block",
                                header { class: "artifact-block-header",
                                    h3 { "{name}" }
                                    span { class: "artifact-format", "{format}" }
                                }
                                match content {
                                    None => rsx! {
                                        p { class: "artifact-loading", role: "status", "aria-live": "polite",
                                            "Loading {name}…"
                                        }
                                    },
                                    Some(Ok(text)) => {
                                        if format.eq_ignore_ascii_case("markdown") {
                                            rsx! { MarkdownView { content: text, label: format!("{name} content") } }
                                        } else {
                                            rsx! { pre { class: "artifact-content", "aria-label": "{name} content", "{text}" } }
                                        }
                                    },
                                    Some(Err(reason)) => rsx! {
                                        p { class: "artifact-error", role: "alert",
                                            "{name} could not be displayed: {reason} Use Download to retry the export."
                                        }
                                    },
                                }
                                div { class: "artifact-actions",
                                    button {
                                        r#type: "button",
                                        class: "secondary",
                                        disabled: copy_text.is_none(),
                                        onclick: move |_| {
                                            let copy_text = copy_text.clone();
                                            let name = name.clone();
                                            spawn(async move {
                                                let Some(text) = copy_text else {
                                                    return;
                                                };
                                                if copy_to_clipboard(&text).await {
                                                    copy_notice.set(Some(format!("{name} copied to the clipboard.")));
                                                } else {
                                                    copy_notice.set(Some(format!(
                                                        "{name} could not be copied. Select the text above and copy it manually."
                                                    )));
                                                }
                                            });
                                        },
                                        "{copy_label}"
                                    }
                                    button {
                                        class: "primary",
                                        onclick: move |_| {
                                            let credential = credential.clone();
                                            let artifact_url = artifact_url.clone();
                                            let filename = filename.clone();
                                            spawn(async move {
                                                match download_artifact(&credential, &artifact_url).await {
                                                    Ok(bytes) => trigger_download(&filename, &bytes),
                                                    Err(message) => {
                                                        workbench_artifacts.with_mut(|artifacts| {
                                                            if let Some(slot) = artifacts.get_mut(index) {
                                                                slot.content = Some(Err(message));
                                                            }
                                                        });
                                                    }
                                                }
                                            });
                                        },
                                        "{download_label}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
