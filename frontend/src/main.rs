use base64::Engine;
use dioxus::prelude::*;
use js_sys::{Array, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, BlobPropertyBag, FormData as WebFormData, Headers, HtmlAnchorElement, HtmlElement,
    Request, RequestInit, RequestMode, Response as WebResponse, Url,
};

/// Must match `backend::contract::MAX_FILE_BYTES`.
const MAX_FILE_BYTES: u64 = 1_048_576;
/// Must match `backend::combine::DIAGNOSTICS_CAPABILITY_HEADER`.
const DIAGNOSTICS_CAPABILITY_HEADER: &str = "x-diagnostics-capability";

fn main() {
    dioxus::launch(App);
}

/// Mirrors `backend::diagnostics::DiagnosticEvent`'s wire shape (minus
/// `request_id`, which the UI has no use for).
#[derive(Clone, PartialEq, serde::Deserialize)]
struct DiagnosticEventView {
    kind: String,
    level: String,
    timestamp_ms: u64,
    message: String,
}

/// Mirrors `backend::diagnostics::RequestDiagnostics`.
#[derive(Clone, PartialEq, serde::Deserialize)]
struct DiagnosticsView {
    events: Vec<DiagnosticEventView>,
    truncated: bool,
}

/// An editable source: pasted or typed text, optionally backed by a loaded
/// file name. The editor content, not the original file bytes, is submitted.
#[derive(Clone, PartialEq, Default)]
struct SourceState {
    name: Option<String>,
    content: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CredentialMode {
    SharedKey,
    Group,
    DirectPat,
}

#[derive(Clone, PartialEq)]
enum Credential {
    SharedKey(String),
    Group { name: String, password: String },
    DirectPat(String),
}

impl Credential {
    fn is_complete(&self) -> bool {
        match self {
            Credential::SharedKey(key) | Credential::DirectPat(key) => !key.is_empty(),
            Credential::Group { name, password } => !name.is_empty() && !password.is_empty(),
        }
    }

    fn authorization_header(&self) -> String {
        match self {
            Credential::SharedKey(key) | Credential::DirectPat(key) => format!("Bearer {key}"),
            Credential::Group { name, password } => {
                let encoded =
                    base64::engine::general_purpose::STANDARD.encode(format!("{name}:{password}"));
                format!("Group {encoded}")
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    AccessValidation,
    FileReading,
    Combining,
    PreparingDownload,
}

impl Stage {
    const ALL: [Stage; 4] = [
        Stage::AccessValidation,
        Stage::FileReading,
        Stage::Combining,
        Stage::PreparingDownload,
    ];

    fn label(self) -> &'static str {
        match self {
            Stage::AccessValidation => "Access validation",
            Stage::FileReading => "File reading",
            Stage::Combining => "Combining",
            Stage::PreparingDownload => "Preparing download",
        }
    }

    fn index(self) -> usize {
        Stage::ALL
            .iter()
            .position(|s| *s == self)
            .expect("stage is in ALL")
    }
}

#[derive(Clone, PartialEq)]
struct CombineResult {
    preview: String,
    bytes: Vec<u8>,
}

#[derive(Clone, PartialEq)]
enum RequestState {
    Idle,
    Submitting(Stage),
    Success(CombineResult),
    Failed(String),
}

/// Awaiting this hands control back to the browser for one render/paint
/// cycle before continuing, so a signal write (e.g. showing the "Submitting"
/// status panel) reaches the DOM even if the next awaited call resolves
/// fast enough to otherwise coalesce into the same render.
async fn next_frame() {
    let promise = Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ = window.request_animation_frame(&resolve);
        }
    });
    let _ = JsFuture::from(promise).await;
}

/// Normalizes line endings to LF and drops non-printable control characters
/// (other than tab/newline) silently, so editors and previews never show a
/// placeholder glyph for content that isn't actually there.
fn normalize_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect()
}

fn focus_element(id: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(element) = document.get_element_by_id(id) else {
        return;
    };
    if let Ok(html_element) = element.dyn_into::<HtmlElement>() {
        let _ = html_element.focus();
    }
}

/// Clears a native `<input type="file">` element's selection, since setting
/// application state alone does not reset the browser's own file-input value.
fn clear_file_input(id: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(element) = document.get_element_by_id(id) else {
        return;
    };
    if let Ok(input) = element.dyn_into::<web_sys::HtmlInputElement>() {
        input.set_value("");
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

    let on_submit = move |_| {
        let credential_value = credential.clone();
        let first = first_source.read().clone();
        let second = second_source.read().clone();
        spawn(async move {
            state.set(RequestState::Submitting(Stage::AccessValidation));
            state.set(RequestState::Submitting(Stage::FileReading));
            state.set(RequestState::Submitting(Stage::Combining));
            next_frame().await;
            diagnostics.set(None);
            let (result, capability) = submit_combine(&credential_value, &first, &second).await;
            match result {
                Ok(bytes) => {
                    state.set(RequestState::Submitting(Stage::PreparingDownload));
                    match std::str::from_utf8(&bytes) {
                        Ok(text) => state.set(RequestState::Success(CombineResult {
                            preview: normalize_text(text),
                            bytes,
                        })),
                        Err(_) => {
                            state.set(RequestState::Failed(
                                "Received an unexpected response".to_string(),
                            ));
                        }
                    }
                }
                Err(message) => state.set(RequestState::Failed(message)),
            }
            if let Some(capability) = capability {
                if let Ok(fetched) = fetch_diagnostics(&credential_value, &capability).await {
                    diagnostics.set(Some(fetched));
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
        clear_file_input("first-file");
        clear_file_input("second-file");
        focus_element("token-input");
    };

    let on_download = move |_| {
        if let RequestState::Success(result) = &*state.read() {
            trigger_download(&result.bytes);
        }
        credential_mode.set(CredentialMode::SharedKey);
        shared_key.set(String::new());
        group_name.set(String::new());
        group_password.set(String::new());
        direct_pat.set(String::new());
        first_source.set(SourceState::default());
        second_source.set(SourceState::default());
        state.set(RequestState::Idle);
        diagnostics.set(None);
        clear_file_input("first-file");
        clear_file_input("second-file");
    };

    rsx! {
        div { class: "app-shell",
            main { class: "workspace",
                header { class: "workspace-header",
                    h1 { "file-combiner" }
                    p { "Choose an access method, then paste or upload a sales guide template and customer information to generate a tailored guide." }
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
                    FileSlot { label: "Sales guide template", slot_id: "first-file", source: first_source }
                    FileSlot { label: "Customer information", slot_id: "second-file", source: second_source }
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
                    if let RequestState::Failed(message) = &*state.read() {
                        p { class: "request-error", role: "alert", "{message}" }
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
                if let RequestState::Success(result) = state.read().clone() {
                    section { class: "result-panel", role: "region", "aria-label": "Combined result",
                        h2 { "Personalized guide" }
                        div {
                            class: "file-preview",
                            tabindex: "0",
                            role: "region",
                            "aria-label": "Combined result preview",
                            pre { "{result.preview}" }
                        }
                        button { class: "primary", onclick: on_download, "Download guide" }
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

#[component]
fn FileSlot(label: &'static str, slot_id: &'static str, source: Signal<SourceState>) -> Element {
    let mut source = source;
    let mut error = use_signal(|| Option::<String>::None);
    let error_id = format!("{slot_id}-error");
    let editor_id = format!("{slot_id}-editor");

    let on_change = move |evt: Event<FormData>| {
        let Some(file_engine) = evt.files() else {
            return;
        };
        spawn(async move {
            let Some(name) = file_engine.files().into_iter().next() else {
                return;
            };
            let Some(size) = file_engine.file_size(&name).await else {
                error.set(Some("Could not read file".to_string()));
                return;
            };
            if size > MAX_FILE_BYTES {
                error.set(Some("File exceeds the 1 MiB limit".to_string()));
                return;
            }
            let Some(bytes) = file_engine.read_file(&name).await else {
                error.set(Some("Could not read file".to_string()));
                return;
            };
            match std::str::from_utf8(&bytes) {
                Ok(text) => {
                    error.set(None);
                    source.set(SourceState {
                        name: Some(name),
                        content: normalize_text(text),
                    });
                }
                Err(_) => {
                    error.set(Some("File is not valid UTF-8 text".to_string()));
                }
            }
        });
    };

    let on_input = move |evt: Event<FormData>| {
        let normalized = normalize_text(&evt.value());
        source.write().content = normalized;
    };

    rsx! {
        div { class: "file-slot",
            label { r#for: "{editor_id}", "{label}" }
            textarea {
                id: "{editor_id}",
                class: "source-editor",
                value: "{source.read().content}",
                oninput: on_input,
            }
            div { class: "file-slot-footer",
                input {
                    id: "{slot_id}",
                    r#type: "file",
                    accept: "text/plain,text/markdown,.txt,.md",
                    onchange: on_change,
                }
                if let Some(name) = source.read().name.clone() {
                    span { class: "file-meta", "{name}" }
                }
            }
            if let Some(message) = error.read().clone() {
                span { id: "{error_id}", class: "file-error", role: "alert", "{message}" }
            }
        }
    }
}

async fn submit_combine(
    credential: &Credential,
    first: &SourceState,
    second: &SourceState,
) -> (Result<Vec<u8>, String>, Option<String>) {
    match submit_combine_inner(credential, first, second).await {
        Ok((bytes, capability)) => (Ok(bytes), capability),
        Err((message, capability)) => (Err(message), capability),
    }
}

async fn submit_combine_inner(
    credential: &Credential,
    first: &SourceState,
    second: &SourceState,
) -> Result<(Vec<u8>, Option<String>), (String, Option<String>)> {
    let form = WebFormData::new().map_err(|_| ("Failed to build the request".to_string(), None))?;
    append_file(&form, "first", first).map_err(|message| (message, None))?;
    append_file(&form, "second", second).map_err(|message| (message, None))?;

    let headers = Headers::new().map_err(|_| ("Failed to build the request".to_string(), None))?;
    headers
        .set("Authorization", &credential.authorization_header())
        .map_err(|_| ("Failed to build the request".to_string(), None))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::SameOrigin);
    opts.set_headers(&headers);
    opts.set_body(&form);

    let request = Request::new_with_str_and_init("/api/combine", &opts)
        .map_err(|_| ("Failed to build the request".to_string(), None))?;

    let window = web_sys::window().ok_or(("No browser window available".to_string(), None))?;
    let response_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| ("Network request failed".to_string(), None))?;
    let response: WebResponse = response_value
        .dyn_into()
        .map_err(|_| ("Unexpected response".to_string(), None))?;

    let capability = response
        .headers()
        .get(DIAGNOSTICS_CAPABILITY_HEADER)
        .ok()
        .flatten();

    if response.ok() {
        let buffer = JsFuture::from(response.array_buffer().map_err(|_| {
            (
                "Failed to read the response".to_string(),
                capability.clone(),
            )
        })?)
        .await
        .map_err(|_| {
            (
                "Failed to read the response".to_string(),
                capability.clone(),
            )
        })?;
        Ok((Uint8Array::new(&buffer).to_vec(), capability))
    } else {
        let json = JsFuture::from(
            response
                .json()
                .map_err(|_| ("Request failed".to_string(), capability.clone()))?,
        )
        .await
        .map_err(|_| ("Request failed".to_string(), capability.clone()))?;
        let message = Reflect::get(&json, &JsValue::from_str("message"))
            .ok()
            .and_then(|value| value.as_string())
            .unwrap_or_else(|| "Request failed".to_string());
        Err((message, capability))
    }
}

/// Fetches the diagnostics recorded for one combine request via its opaque
/// capability, using the same credential as the original request.
async fn fetch_diagnostics(
    credential: &Credential,
    capability: &str,
) -> Result<DiagnosticsView, String> {
    let headers = Headers::new().map_err(|_| "Failed to build the request".to_string())?;
    headers
        .set("Authorization", &credential.authorization_header())
        .map_err(|_| "Failed to build the request".to_string())?;

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::SameOrigin);
    opts.set_headers(&headers);

    let url = format!("/api/diagnostics/{capability}");
    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|_| "Failed to build the request".to_string())?;

    let window = web_sys::window().ok_or("No browser window available")?;
    let response_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| "Network request failed".to_string())?;
    let response: WebResponse = response_value
        .dyn_into()
        .map_err(|_| "Unexpected response".to_string())?;

    if !response.ok() {
        return Err("Failed to load diagnostics".to_string());
    }

    let text_value = JsFuture::from(
        response
            .text()
            .map_err(|_| "Failed to read diagnostics".to_string())?,
    )
    .await
    .map_err(|_| "Failed to read diagnostics".to_string())?;
    let text = text_value
        .as_string()
        .ok_or_else(|| "Failed to read diagnostics".to_string())?;

    serde_json::from_str(&text).map_err(|_| "Failed to parse diagnostics".to_string())
}

fn append_file(form: &WebFormData, field: &str, source: &SourceState) -> Result<(), String> {
    let bytes = source.content.as_bytes();
    let array = Uint8Array::from(bytes);
    let parts = Array::new();
    parts.push(&array);

    let blob_options = BlobPropertyBag::new();
    blob_options.set_type("text/plain");
    let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &blob_options)
        .map_err(|_| "Failed to build the request".to_string())?;

    let name = source
        .name
        .clone()
        .unwrap_or_else(|| format!("{field}.txt"));
    form.append_with_blob_and_filename(field, &blob, &name)
        .map_err(|_| "Failed to build the request".to_string())
}

fn trigger_download(bytes: &[u8]) {
    let array = Uint8Array::from(bytes);
    let parts = Array::new();
    parts.push(&array);

    let blob_options = BlobPropertyBag::new();
    blob_options.set_type("text/plain;charset=utf-8");
    let Ok(blob) = Blob::new_with_u8_array_sequence_and_options(&parts, &blob_options) else {
        return;
    };
    let Ok(url) = Url::create_object_url_with_blob(&blob) else {
        return;
    };

    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Ok(anchor) = document.create_element("a") {
                if let Ok(anchor) = anchor.dyn_into::<HtmlAnchorElement>() {
                    anchor.set_href(&url);
                    anchor.set_download("combined.txt");
                    anchor.click();
                }
            }
        }
    }

    let _ = Url::revoke_object_url(&url);
}
