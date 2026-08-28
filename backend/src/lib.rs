pub mod combinator_agent;
pub mod combine;
pub mod config;
pub mod contract;
pub mod diagnostics;

use axum::{routing::get, Router};
pub use combine::AppState;
use tower_http::services::{ServeDir, ServeFile};

/// Builds the application router: the unauthenticated health check, the
/// authenticated combine endpoint, and same-origin static frontend serving
/// for every other path.
pub fn router(state: AppState, static_dir: &str) -> Router {
    let serve_dir = ServeDir::new(static_dir)
        .not_found_service(ServeFile::new(format!("{static_dir}/index.html")));
    Router::new()
        .route("/healthz", get(healthz))
        .merge(combine::routes(state))
        .fallback_service(serve_dir)
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
        AppState::new(
            "bearer-token".to_string(),
            "probe-token".to_string(),
            std::sync::Arc::new(combinator_agent::FakeCombineBackend),
        )
    }

    #[tokio::test]
    async fn healthz_returns_200_without_exposing_content() {
        let app = router(test_state(), "frontend/dist");
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
