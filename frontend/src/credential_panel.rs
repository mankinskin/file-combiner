use dioxus::prelude::*;

use crate::models::CredentialMode;

/// Renders the access-method selector and the credential inputs for the
/// currently selected mode.
#[component]
pub(crate) fn CredentialPanel(
    credential_mode: Signal<CredentialMode>,
    shared_key: Signal<String>,
    group_name: Signal<String>,
    group_password: Signal<String>,
    direct_pat: Signal<String>,
    busy: bool,
) -> Element {
    let mut credential_mode = credential_mode;
    let mut shared_key = shared_key;
    let mut group_name = group_name;
    let mut group_password = group_password;
    let mut direct_pat = direct_pat;

    rsx! {
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
}
