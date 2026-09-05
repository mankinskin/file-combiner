use js_sys::{Array, Promise, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, HtmlElement, Url};

pub(crate) async fn next_frame() {
    let promise = Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ = window.request_animation_frame(&resolve);
        }
    });
    let _ = JsFuture::from(promise).await;
}

pub(crate) fn focus_element(id: &str) {
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

pub(crate) fn clear_file_input(id: &str) {
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

pub(crate) fn trigger_download(filename: &str, bytes: &[u8]) {
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
                    anchor.set_download(filename);
                    anchor.click();
                }
            }
        }
    }

    let _ = Url::revoke_object_url(&url);
}
