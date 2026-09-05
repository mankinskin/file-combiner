use dioxus::prelude::*;

use crate::{
    models::{SourceState, MAX_FILE_BYTES},
    text::normalize_text,
};

#[component]
pub(crate) fn FileSlot(
    label: &'static str,
    slot_id: &'static str,
    source: Signal<SourceState>,
) -> Element {
    let mut source = source;
    let mut error = use_signal(|| Option::<String>::None);
    let error_id = format!("{slot_id}-error");
    let editor_id = format!("{slot_id}-editor");

    let on_change = move |event: Event<FormData>| {
        let Some(file_engine) = event.files() else {
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
                Err(_) => error.set(Some("File is not valid UTF-8 text".to_string())),
            }
        });
    };

    let on_input = move |event: Event<FormData>| {
        source.write().content = normalize_text(&event.value());
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
