//! Diagnostics-capability issuance, redaction, and expiry tests.
use super::*;

fn diagnostics_request(auth: Option<&str>, capability: &str) -> Request<Body> {
    let mut builder = Request::builder()
        .method("GET")
        .uri(format!("/api/diagnostics/{capability}"));
    if let Some(auth) = auth {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {auth}"));
    }
    builder.body(Body::empty()).unwrap()
}

async fn diagnostics_body(response: axum::response::Response) -> Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

/// End-to-end: a combine request issues a diagnostics capability that a
/// bearer-authenticated caller can use to repeatedly retrieve bounded,
/// structured, redacted records correlated to that request and only that
/// request, until expiry.
#[tokio::test(start_paused = true)]
async fn combine_issues_diagnostics_capability_with_structured_redacted_records() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[
        file_part("first", "first.txt", VALID_TEMPLATE),
        file_part("second", "second.txt", "world"),
    ]);

    let response = app
        .clone()
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let capability = response
        .headers()
        .get(backend::combine::DIAGNOSTICS_CAPABILITY_HEADER)
        .expect("combine response must carry a diagnostics capability")
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        capability.len(),
        64,
        "capability must be a 256-bit hex token"
    );

    // Repeat reads within the TTL succeed and return the same records.
    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(diagnostics_request(Some(BEARER_TOKEN), &capability))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let value = diagnostics_body(response).await;
        let events = value.get("events").unwrap().as_array().unwrap();
        assert!(!events.is_empty());

        for event in events {
            let object = event.as_object().unwrap();
            assert!(object.contains_key("request_id"));
            assert!(object.contains_key("kind"));
            assert!(object.contains_key("level"));
            assert!(object.contains_key("timestamp_ms"));
            let message = object.get("message").unwrap().as_str().unwrap();
            assert!(
                !message.contains("retirement planning"),
                "message must not contain upload content"
            );
            assert!(
                !message.contains("world"),
                "message must not contain upload content"
            );
            assert!(!message.contains(BEARER_TOKEN));
            assert!(!message.contains(PROBE_TOKEN));
        }
    }

    // A direct PAT caller can also read a capability-bearing diagnostics
    // response; otherwise Direct PAT failures lose their backend trace.
    let response = app
        .clone()
        .oneshot(diagnostics_request(Some("ghp_test-token"), &capability))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // An unrelated, well-formed capability never matches this request's records.
    let response = app
        .clone()
        .oneshot(diagnostics_request(
            Some(BEARER_TOKEN),
            "0".repeat(64).as_str(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Expiry: after the 15-minute retention window the capability is gone.
    tokio::time::advance(std::time::Duration::from_secs(15 * 60 + 1)).await;
    let response = app
        .clone()
        .oneshot(diagnostics_request(Some(BEARER_TOKEN), &capability))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
