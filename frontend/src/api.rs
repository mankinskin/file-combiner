use js_sys::{Array, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, BlobPropertyBag, FormData, Headers, Request, RequestInit, RequestMode,
    Response as WebResponse,
};

use crate::models::{ArtifactList, Credential, DiagnosticsView, SourceState};

const DIAGNOSTICS_CAPABILITY_HEADER: &str = "x-diagnostics-capability";

pub(crate) async fn submit_combine(
    credential: &Credential,
    first: &SourceState,
    second: &SourceState,
) -> (Result<ArtifactList, String>, Option<String>) {
    match submit_combine_inner(credential, first, second).await {
        Ok((bytes, capability)) => (Ok(bytes), capability),
        Err((message, capability)) => (Err(message), capability),
    }
}

async fn submit_combine_inner(
    credential: &Credential,
    first: &SourceState,
    second: &SourceState,
) -> Result<(ArtifactList, Option<String>), (String, Option<String>)> {
    let form = FormData::new().map_err(|_| ("Failed to build the request".to_string(), None))?;
    append_file(&form, "first", first).map_err(|message| (message, None))?;
    append_file(&form, "second", second).map_err(|message| (message, None))?;
    let headers = authorization_headers(credential).map_err(|message| (message, None))?;

    let options = RequestInit::new();
    options.set_method("POST");
    options.set_mode(RequestMode::SameOrigin);
    options.set_headers(&headers);
    options.set_body(&form);
    let request = Request::new_with_str_and_init("/api/combine", &options)
        .map_err(|_| ("Failed to build the request".to_string(), None))?;
    let response = send_request(&request)
        .await
        .map_err(|message| (message, None))?;
    let capability = response
        .headers()
        .get(DIAGNOSTICS_CAPABILITY_HEADER)
        .ok()
        .flatten();

    if response.ok() {
        let text_value = JsFuture::from(response.text().map_err(|_| {
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
        let text = text_value.as_string().ok_or_else(|| {
            (
                "Failed to read the response".to_string(),
                capability.clone(),
            )
        })?;
        let manifest = serde_json::from_str(&text).map_err(|_| {
            (
                "Received an unexpected response".to_string(),
                capability.clone(),
            )
        })?;
        Ok((manifest, capability))
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

pub(crate) async fn download_artifact(
    credential: &Credential,
    artifact_url: &str,
) -> Result<Vec<u8>, String> {
    let headers = authorization_headers(credential)?;
    let options = RequestInit::new();
    options.set_method("GET");
    options.set_mode(RequestMode::SameOrigin);
    options.set_headers(&headers);
    let request = Request::new_with_str_and_init(artifact_url, &options)
        .map_err(|_| "Failed to build the download request".to_string())?;
    let response = send_request(&request).await?;
    if !response.ok() {
        return Err("Failed to download the artifact".to_string());
    }
    let buffer = JsFuture::from(
        response
            .array_buffer()
            .map_err(|_| "Failed to read the artifact".to_string())?,
    )
    .await
    .map_err(|_| "Failed to read the artifact".to_string())?;
    Ok(Uint8Array::new(&buffer).to_vec())
}

pub(crate) async fn fetch_diagnostics(
    credential: &Credential,
    capability: &str,
) -> Result<DiagnosticsView, String> {
    let headers = authorization_headers(credential)?;
    let options = RequestInit::new();
    options.set_method("GET");
    options.set_mode(RequestMode::SameOrigin);
    options.set_headers(&headers);
    let url = format!("/api/diagnostics/{capability}");
    let request = Request::new_with_str_and_init(&url, &options)
        .map_err(|_| "Failed to build the request".to_string())?;
    let response = send_request(&request).await?;
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

fn authorization_headers(credential: &Credential) -> Result<Headers, String> {
    let headers = Headers::new().map_err(|_| "Failed to build the request".to_string())?;
    headers
        .set("Authorization", &credential.authorization_header())
        .map_err(|_| "Failed to build the request".to_string())?;
    Ok(headers)
}

async fn send_request(request: &Request) -> Result<WebResponse, String> {
    let window = web_sys::window().ok_or("No browser window available")?;
    let response_value = JsFuture::from(window.fetch_with_request(request))
        .await
        .map_err(|_| "Network request failed".to_string())?;
    response_value
        .dyn_into()
        .map_err(|_| "Unexpected response".to_string())
}

fn append_file(form: &FormData, field: &str, source: &SourceState) -> Result<(), String> {
    let array = Uint8Array::from(source.content.as_bytes());
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
