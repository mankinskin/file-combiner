use dioxus::prelude::*;
use js_sys::{Array, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, BlobPropertyBag, FormData as WebFormData, Headers, HtmlAnchorElement, HtmlElement,
    Request, RequestInit, RequestMode, Response as WebResponse, Url,
};

/// Must match `backend::contract::MAX_FILE_BYTES`.
const MAX_FILE_BYTES: u64 = 1_048_576;

fn main() {
    dioxus::launch(App);
}

#[derive(Clone, PartialEq)]
struct SelectedFile {
    name: String,
    bytes: Vec<u8>,
    preview: String,
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

/// Replaces control characters other than tab/newline with a visible
/// placeholder so previews never hide non-printable content.
fn sanitize_preview(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\n' | '\t' => c,
            c if c.is_control() => '\u{FFFD}',
            c => c,
        })
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

/// Combines two authenticated text uploads through same-origin `/api/combine`.
/// The token and file selections live only in this component's memory; a
/// page reload clears both.
#[component]
fn App() -> Element {
    let mut token = use_signal(String::new);
    let mut first_file = use_signal(|| Option::<SelectedFile>::None);
    let mut second_file = use_signal(|| Option::<SelectedFile>::None);
    let mut state = use_signal(|| RequestState::Idle);

    let busy = matches!(*state.read(), RequestState::Submitting(_));
    let can_submit = !token.read().is_empty()
        && first_file.read().is_some()
        && second_file.read().is_some()
        && !busy;

    let on_submit = move |_| {
        let token_value = token.read().clone();
        let Some(first) = first_file.read().clone() else {
            return;
        };
        let Some(second) = second_file.read().clone() else {
            return;
        };
        spawn(async move {
            state.set(RequestState::Submitting(Stage::AccessValidation));
            state.set(RequestState::Submitting(Stage::FileReading));
            state.set(RequestState::Submitting(Stage::Combining));
            match submit_combine(&token_value, &first, &second).await {
                Ok(bytes) => {
                    state.set(RequestState::Submitting(Stage::PreparingDownload));
                    match std::str::from_utf8(&bytes) {
                        Ok(text) => state.set(RequestState::Success(CombineResult {
                            preview: sanitize_preview(text),
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
        });
    };

    let on_reset = move |_| {
        token.set(String::new());
        first_file.set(None);
        second_file.set(None);
        state.set(RequestState::Idle);
        clear_file_input("first-file");
        clear_file_input("second-file");
        focus_element("token-input");
    };

    let on_download = move |_| {
        if let RequestState::Success(result) = &*state.read() {
            trigger_download(&result.bytes);
        }
        token.set(String::new());
        first_file.set(None);
        second_file.set(None);
        state.set(RequestState::Idle);
        clear_file_input("first-file");
        clear_file_input("second-file");
    };

    rsx! {
        main { class: "workspace",
            header { class: "workspace-header",
                h1 { "file-combiner" }
                p { "Enter the shared access key and choose two ordered text files to combine." }
            }
            section { class: "workspace-body",
                div { class: "action-panel",
                    label { r#for: "token-input", "Access key" }
                    input {
                        id: "token-input",
                        r#type: "password",
                        value: "{token}",
                        disabled: busy,
                        oninput: move |evt| token.set(evt.value()),
                    }
                    div { class: "actions",
                        button {
                            class: "primary",
                            disabled: !can_submit,
                            "aria-busy": if busy { "true" } else { "false" },
                            onclick: on_submit,
                            "Combine"
                        }
                        button {
                            r#type: "button",
                            class: "secondary",
                            disabled: busy,
                            onclick: on_reset,
                            "Reset"
                        }
                    }
                    if let RequestState::Failed(message) = &*state.read() {
                        p { class: "request-error", role: "alert", "{message}" }
                    }
                }
                FileSlot { label: "First file", slot_id: "first-file", selected: first_file }
                FileSlot { label: "Second file", slot_id: "second-file", selected: second_file }
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
                    h2 { "Result" }
                    div {
                        class: "file-preview",
                        tabindex: "0",
                        role: "region",
                        "aria-label": "Combined result preview",
                        pre { "{result.preview}" }
                    }
                    button { class: "primary", onclick: on_download, "Download combined.txt" }
                }
            }
        }
    }
}

#[component]
fn FileSlot(
    label: &'static str,
    slot_id: &'static str,
    selected: Signal<Option<SelectedFile>>,
) -> Element {
    let mut selected = selected;
    let mut error = use_signal(|| Option::<String>::None);
    let error_id = format!("{slot_id}-error");
    let preview_id = format!("{slot_id}-preview");

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
                selected.set(None);
                return;
            };
            if size > MAX_FILE_BYTES {
                error.set(Some("File exceeds the 1 MiB limit".to_string()));
                selected.set(None);
                return;
            }
            let Some(bytes) = file_engine.read_file(&name).await else {
                error.set(Some("Could not read file".to_string()));
                selected.set(None);
                return;
            };
            match std::str::from_utf8(&bytes) {
                Ok(text) => {
                    error.set(None);
                    selected.set(Some(SelectedFile {
                        name,
                        preview: sanitize_preview(text),
                        bytes,
                    }));
                }
                Err(_) => {
                    error.set(Some("File is not valid UTF-8 text".to_string()));
                    selected.set(None);
                }
            }
        });
    };

    rsx! {
        div { class: "file-slot",
            label { r#for: "{slot_id}", "{label}" }
            input {
                id: "{slot_id}",
                r#type: "file",
                accept: "text/plain,.txt",
                onchange: on_change,
            }
            if let Some(file) = selected.read().clone() {
                div {
                    class: "file-preview",
                    id: "{preview_id}",
                    tabindex: "0",
                    role: "region",
                    "aria-label": "{label} preview: {file.name}",
                    pre { "{file.preview}" }
                }
                span { class: "file-meta", "{file.name} ({file.bytes.len()} bytes)" }
            }
            if let Some(message) = error.read().clone() {
                span { id: "{error_id}", class: "file-error", role: "alert", "{message}" }
            }
        }
    }
}

async fn submit_combine(
    token: &str,
    first: &SelectedFile,
    second: &SelectedFile,
) -> Result<Vec<u8>, String> {
    let form = WebFormData::new().map_err(|_| "Failed to build the request".to_string())?;
    append_file(&form, "first", first)?;
    append_file(&form, "second", second)?;

    let headers = Headers::new().map_err(|_| "Failed to build the request".to_string())?;
    headers
        .set("Authorization", &format!("Bearer {token}"))
        .map_err(|_| "Failed to build the request".to_string())?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::SameOrigin);
    opts.set_headers(&headers);
    opts.set_body(&form);

    let request = Request::new_with_str_and_init("/api/combine", &opts)
        .map_err(|_| "Failed to build the request".to_string())?;

    let window = web_sys::window().ok_or("No browser window available")?;
    let response_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| "Network request failed".to_string())?;
    let response: WebResponse = response_value
        .dyn_into()
        .map_err(|_| "Unexpected response".to_string())?;

    if response.ok() {
        let buffer = JsFuture::from(
            response
                .array_buffer()
                .map_err(|_| "Failed to read the response".to_string())?,
        )
        .await
        .map_err(|_| "Failed to read the response".to_string())?;
        Ok(Uint8Array::new(&buffer).to_vec())
    } else {
        let json = JsFuture::from(response.json().map_err(|_| "Request failed".to_string())?)
            .await
            .map_err(|_| "Request failed".to_string())?;
        let message = Reflect::get(&json, &JsValue::from_str("message"))
            .ok()
            .and_then(|value| value.as_string())
            .unwrap_or_else(|| "Request failed".to_string());
        Err(message)
    }
}

fn append_file(form: &WebFormData, field: &str, file: &SelectedFile) -> Result<(), String> {
    let array = Uint8Array::from(file.bytes.as_slice());
    let parts = Array::new();
    parts.push(&array);

    let blob_options = BlobPropertyBag::new();
    blob_options.set_type("text/plain");
    let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &blob_options)
        .map_err(|_| "Failed to build the request".to_string())?;

    form.append_with_blob_and_filename(field, &blob, &file.name)
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
