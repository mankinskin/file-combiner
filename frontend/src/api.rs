use js_sys::{Array, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, BlobPropertyBag, FormData, Headers, Request, RequestInit, RequestMode,
    Response as WebResponse,
};

use crate::models::{AnalysisResponse, ApiErrorBody, Credential, DiagnosticsView, SourceState};

const DIAGNOSTICS_CAPABILITY_HEADER: &str = "x-diagnostics-capability";

pub(crate) async fn submit_analysis(
    credential: &Credential,
    kind: &str,
    source: &SourceState,
    session_id: Option<&str>,
) -> (Result<AnalysisResponse, ApiErrorBody>, Option<String>) {
    match submit_analysis_inner(credential, kind, source, session_id).await {
        Ok((response, capability)) => (Ok(response), capability),
        Err((message, capability)) => (Err(message), capability),
    }
}

async fn submit_analysis_inner(
    credential: &Credential,
    kind: &str,
    source: &SourceState,
    session_id: Option<&str>,
) -> Result<(AnalysisResponse, Option<String>), (ApiErrorBody, Option<String>)> {
    let form =
        FormData::new().map_err(|_| (ApiErrorBody::local("Failed to build the request"), None))?;
    form.append_with_str("kind", kind)
        .map_err(|_| (ApiErrorBody::local("Failed to build the request"), None))?;
    if let Some(session_id) = session_id {
        form.append_with_str("session_id", session_id)
            .map_err(|_| (ApiErrorBody::local("Failed to build the request"), None))?;
    }
    append_file(&form, "artifact", source)
        .map_err(|message| (ApiErrorBody::local(message), None))?;
    let headers = authorization_headers(credential)
        .map_err(|message| (ApiErrorBody::local(message), None))?;
    let options = RequestInit::new();
    options.set_method("POST");
    options.set_mode(RequestMode::SameOrigin);
    options.set_headers(&headers);
    options.set_body(&form);
    let request = Request::new_with_str_and_init("/api/analyze", &options)
        .map_err(|_| (ApiErrorBody::local("Failed to build the request"), None))?;
    let response = send_request(&request)
        .await
        .map_err(|message| (ApiErrorBody::local(message), None))?;
    let capability = response
        .headers()
        .get(DIAGNOSTICS_CAPABILITY_HEADER)
        .ok()
        .flatten();
    let text = JsFuture::from(response.text().map_err(|_| {
        (
            ApiErrorBody::local("Failed to read the analysis response"),
            capability.clone(),
        )
    })?)
    .await
    .map_err(|_| {
        (
            ApiErrorBody::local("Failed to read the analysis response"),
            capability.clone(),
        )
    })?
    .as_string()
    .ok_or_else(|| {
        (
            ApiErrorBody::local("Failed to read the analysis response"),
            capability.clone(),
        )
    })?;
    if !response.ok() {
        let status = response.status();
        let error = serde_json::from_str::<ApiErrorBody>(&text).unwrap_or_else(|_| {
            ApiErrorBody::from_response(
                status,
                "Request failed",
                vec![format!("Raw response body: {text}")],
            )
        });
        return Err((error, capability));
    }
    let parsed = serde_json::from_str(&text).map_err(|_| {
        (
            ApiErrorBody::from_response(
                response.status(),
                "Received an unexpected analysis response",
                vec![format!("Raw response body: {text}")],
            ),
            capability.clone(),
        )
    })?;
    Ok((parsed, capability))
}

pub(crate) async fn fetch_analysis_diagnostics(
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
