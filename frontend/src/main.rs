use dioxus::prelude::*;
use js_sys::{Array, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, BlobPropertyBag, FormData as WebFormData, Headers, HtmlAnchorElement, Request,
    RequestInit, RequestMode, Response as WebResponse, Url,
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
}

/// Combines two authenticated text uploads through same-origin `/api/combine`.
/// The token and file selections live only in this component's memory; a
/// page reload clears both.
#[component]
fn App() -> Element {
    let mut token = use_signal(String::new);
    let first_file = use_signal(|| Option::<SelectedFile>::None);
    let second_file = use_signal(|| Option::<SelectedFile>::None);
    let mut status = use_signal(|| Option::<String>::None);
    let mut busy = use_signal(|| false);

    let can_submit = !token.read().is_empty()
        && first_file.read().is_some()
        && second_file.read().is_some()
        && !*busy.read();

    let on_submit = move |_| {
        let token = token.read().clone();
        let Some(first) = first_file.read().clone() else {
            return;
        };
        let Some(second) = second_file.read().clone() else {
            return;
        };
        spawn(async move {
            busy.set(true);
            status.set(None);
            match submit_combine(&token, &first, &second).await {
                Ok(bytes) => {
                    trigger_download(&bytes);
                    status.set(Some("Downloaded combined.txt".to_string()));
                }
                Err(message) => status.set(Some(message)),
            }
            busy.set(false);
        });
    };

    rsx! {
        main {
            h1 { "file-combiner" }
            p { "Enter the shared trusted-group token and choose two text files in order." }
            label {
                "Token"
                input {
                    r#type: "password",
                    value: "{token}",
                    oninput: move |evt| token.set(evt.value()),
                }
            }
            FileSlot { label: "First file", selected: first_file }
            FileSlot { label: "Second file", selected: second_file }
            button {
                disabled: !can_submit,
                onclick: on_submit,
                "Combine and download"
            }
            if let Some(message) = status.read().clone() {
                p { "{message}" }
            }
        }
    }
}

#[component]
fn FileSlot(label: &'static str, selected: Signal<Option<SelectedFile>>) -> Element {
    let mut selected = selected;
    let mut error = use_signal(|| Option::<String>::None);

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
                selected.set(None);
                return;
            }
            let Some(bytes) = file_engine.read_file(&name).await else {
                error.set(Some("Could not read file".to_string()));
                return;
            };
            error.set(None);
            selected.set(Some(SelectedFile { name, bytes }));
        });
    };

    rsx! {
        label {
            "{label}"
            input { r#type: "file", onchange: on_change }
        }
        if let Some(file) = selected.read().clone() {
            span { " {file.name} ({file.bytes.len()} bytes)" }
        }
        if let Some(message) = error.read().clone() {
            span { " {message}" }
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
