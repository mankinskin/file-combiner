pub mod combine;
pub mod config;
pub mod contract;

use axum::{routing::get, Router};
pub use combine::AppState;

/// Builds the application router: the unauthenticated health check plus the
/// authenticated combine endpoint. The static SPA fallback lands in a later package.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .merge(combine::routes(state))
}

async fn healthz() -> &'static str {
    "ok"
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        AppState::new("bearer-token".to_string(), "probe-token".to_string())
    }

    #[tokio::test]
    async fn healthz_returns_200_without_exposing_content() {
        let app = router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"ok");
    }
}
