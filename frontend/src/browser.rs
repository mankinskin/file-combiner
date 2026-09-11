use js_sys::{Array, Promise, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, HtmlElement, Url};

/// Yields one macrotask so a UI update (e.g. "Processing") can repaint
/// before the next step runs. Uses `setTimeout`, not `requestAnimationFrame`:
/// browsers suspend rAF entirely on a hidden/backgrounded page, which would
/// hang this indefinitely and block the request that follows it.
pub(crate) async fn next_frame() {
    let promise = Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 0);
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

/// Copies artifact text to the clipboard so a user can take a completed
/// block without downloading it. Returns false when the browser withholds
/// the clipboard (insecure origin or denied permission) so the caller can
/// surface an actionable state instead of silently doing nothing.
pub(crate) async fn copy_to_clipboard(text: &str) -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let clipboard = window.navigator().clipboard();
    JsFuture::from(clipboard.write_text(text)).await.is_ok()
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
