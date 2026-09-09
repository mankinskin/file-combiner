use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use backend::combinator_agent::{
    latent::{ConversationStage, CustomerContext, SourceKind, TargetAspect, LATENT_SCHEMA_VERSION},
    LatentRepresentation, MockCombineBackend,
};
use backend::{router, AppState};
use http_body_util::BodyExt;
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

const BOUNDARY: &str = "analyze-test-boundary";

fn multipart(parts: &[(&str, Option<&str>, &str)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, filename, content) in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        if let Some(filename) = filename {
            body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: text/plain\r\n\r\n"
                )
                .as_bytes(),
            );
        } else {
            body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
            );
        }
        body.extend_from_slice(content.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

fn request(body: Vec<u8>) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/analyze")
        .header(header::AUTHORIZATION, "Bearer test-token")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(body))
        .unwrap()
}

fn state() -> AppState {
    AppState::new(
        "test-token".to_owned(),
        "probe-token".to_owned(),
        Arc::new(MockCombineBackend::from_analysis_fixtures(vec![
            fixture(SourceKind::Template, "fixture-template"),
            fixture(SourceKind::PersonProfile, "fixture-profile"),
        ])),
    )
}

fn fixture(source_kind: SourceKind, title: &str) -> LatentRepresentation {
    LatentRepresentation {
        schema_version: LATENT_SCHEMA_VERSION,
        source_kind,
        title: title.to_owned(),
        domain: Some("sales".to_owned()),
        stages: vec![ConversationStage {
            name: "Discovery".to_owned(),
            purpose: Some("Understand the situation".to_owned()),
            questions: vec!["What matters most?".to_owned()],
            information: vec!["Fixture information".to_owned()],
            target_aspects: vec![TargetAspect::Customer],
        }],
        customer: CustomerContext {
            situation: vec!["Fixture situation".to_owned()],
            goals: vec!["Fixture goal".to_owned()],
            ..CustomerContext::default()
        },
        motivations: vec!["Fixture motivation".to_owned()],
        questions: vec!["Fixture question?".to_owned()],
        objections: vec!["Fixture objection".to_owned()],
        tone: vec!["clear".to_owned()],
        source_spans: vec![],
    }
}

fn unauthorized_request(body: Vec<u8>) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/analyze")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(body))
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn analysis_returns_partial_then_complete_session_with_downloads() {
    let app = router(state(), "frontend/dist");
    let response = app
        .clone()
        .oneshot(request(multipart(&[
            ("kind", None, "template"),
            ("artifact", Some("closing.md"), "# Opening\nAre you ready?"),
        ])))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let partial: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(partial["complete"], false);
    assert_eq!(partial["representations"].as_array().unwrap().len(), 1);
    let session_id = partial["session_id"].as_str().unwrap().to_owned();

    let response = app
        .clone()
        .oneshot(request(multipart(&[
            ("kind", None, "profile"),
            ("session_id", None, &session_id),
            ("artifact", Some("person.md"), "# Goals\n- Build cashflow"),
        ])))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let complete: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(complete["complete"], true);
    assert_eq!(complete["artifacts"].as_array().unwrap().len(), 2);

    let download = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/sessions/{session_id}/artifacts/artifact-0"))
                .header(header::AUTHORIZATION, "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(download.status(), StatusCode::OK);
    let bytes = download.into_body().collect().await.unwrap().to_bytes();
    assert!(std::str::from_utf8(&bytes)
        .unwrap()
        .contains("Closer overview"));
}

#[tokio::test]
async fn analysis_rejects_missing_credentials_and_required_fields() {
    let app = router(state(), "frontend/dist");
    let body = multipart(&[("kind", None, "template")]);
    let response = app
        .clone()
        .oneshot(unauthorized_request(body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = app
        .clone()
        .oneshot(request(multipart(&[("kind", None, "template")])))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(response).await["code"], "wrong_file_count");

    let response = app
        .oneshot(request(multipart(&[(
            "artifact",
            Some("template.md"),
            "# Template",
        )])))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(response).await["code"], "invalid_field_name");
}

#[tokio::test]
async fn analysis_rejects_invalid_kind_unknown_fields_and_unknown_sessions() {
    let app = router(state(), "frontend/dist");
    let response = app
        .clone()
        .oneshot(request(multipart(&[
            ("kind", None, "unknown"),
            ("artifact", Some("template.md"), "# Template"),
        ])))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(response).await["code"], "invalid_field_name");

    let response = app
        .clone()
        .oneshot(request(multipart(&[
            ("kind", None, "template"),
            ("unexpected", None, "value"),
            ("artifact", Some("template.md"), "# Template"),
        ])))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(response).await["code"], "invalid_field_name");

    let response = app
        .oneshot(request(multipart(&[
            ("kind", None, "profile"),
            ("session_id", None, "missing-session"),
            ("artifact", Some("profile.md"), "# Profile"),
        ])))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(json_body(response).await["code"], "not_found");
}

#[tokio::test]
async fn session_artifact_download_requires_auth_and_rejects_unknown_ids() {
    let app = router(state(), "frontend/dist");
    let response = app
        .clone()
        .oneshot(request(multipart(&[
            ("kind", None, "template"),
            ("artifact", Some("template.md"), "# Template"),
        ])))
        .await
        .unwrap();
    let partial = json_body(response).await;
    let session_id = partial["session_id"].as_str().unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/sessions/{session_id}/artifacts/artifact-0"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/sessions/{session_id}/artifacts/unknown"))
                .header(header::AUTHORIZATION, "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn analysis_accepts_profile_before_template() {
    let app = router(state(), "frontend/dist");
    let response = app
        .clone()
        .oneshot(request(multipart(&[
            ("kind", None, "profile"),
            (
                "artifact",
                Some("profile.md"),
                "# Goals\n- Improve operations",
            ),
        ])))
        .await
        .unwrap();
    let partial = json_body(response).await;
    assert!(!partial["complete"].as_bool().unwrap());
    let session_id = partial["session_id"].as_str().unwrap().to_owned();

    let response = app
        .oneshot(request(multipart(&[
            ("kind", None, "template"),
            ("session_id", None, &session_id),
            (
                "artifact",
                Some("template.md"),
                "# Discovery\nWhat matters most?",
            ),
        ])))
        .await
        .unwrap();
    let complete = json_body(response).await;
    assert!(complete["complete"].as_bool().unwrap());
    assert_eq!(complete["representations"].as_array().unwrap().len(), 2);
    assert_eq!(complete["artifacts"].as_array().unwrap().len(), 2);
}
