//! Manifest generation, per-artifact download, and default-artifact tests.
use super::*;

#[tokio::test]
async fn combine_returns_manifest_and_each_artifact_downloads_individually() {
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
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let manifest: Value = serde_json::from_slice(&body).unwrap();
    let artifacts = manifest.get("artifacts").unwrap().as_array().unwrap();
    assert_eq!(artifacts.len(), 2);
    assert_eq!(artifacts[0].get("id").unwrap(), "artifact-0");
    assert_eq!(artifacts[0].get("name").unwrap(), "preparation-brief");
    assert_eq!(artifacts[1].get("id").unwrap(), "artifact-1");
    assert_eq!(artifacts[1].get("name").unwrap(), "question-list");

    for artifact in artifacts {
        let url = artifact.get("download_url").unwrap().as_str().unwrap();
        let response = app
            .clone()
            .oneshot(download_request(Some(BEARER_TOKEN), url))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/plain; charset=utf-8"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let name = artifact.get("name").unwrap().as_str().unwrap();
        assert_eq!(&body[..], format!("# {name}\n\nworld").as_bytes());
    }

    let url = artifacts[0].get("download_url").unwrap().as_str().unwrap();
    let response = app.oneshot(download_request(None, url)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn template_without_artifacts_uses_default_artifact() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[
        file_part(
            "first",
            "first.txt",
            VALID_TEMPLATE.replace(
                "artifacts:\n  - name: preparation-brief\n    format: markdown\n    sections:\n      - Priorities\n",
                "artifacts: []\n",
            ),
        ),
        file_part("second", "second.txt", "world"),
    ]);

    let response = app
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let manifest: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(manifest["artifacts"][0]["name"], "preparation-brief");
    assert_eq!(manifest["artifacts"][0]["format"], "md");
}
