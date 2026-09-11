use super::*;

#[test]
fn rejects_missing_bearer_token() {
    let err = AppConfig::from_values(
        None,
        Some("probe".into()),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(err, ConfigError::MissingBearerToken);
}

#[test]
fn rejects_empty_bearer_token() {
    let err = AppConfig::from_values(
        Some(String::new()),
        Some("probe".into()),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(err, ConfigError::MissingBearerToken);
}

#[test]
fn rejects_missing_probe_token() {
    let err = AppConfig::from_values(
        Some("bearer".into()),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(err, ConfigError::MissingProbeToken);
}

#[test]
fn rejects_equal_tokens() {
    let err = AppConfig::from_values(
        Some("same".into()),
        Some("same".into()),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(err, ConfigError::TokensMustDiffer);
}

#[test]
fn rejects_invalid_port() {
    let err = AppConfig::from_values(
        Some("bearer".into()),
        Some("probe".into()),
        Some("not-a-port".into()),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert!(matches!(err, ConfigError::InvalidPort(_)));
}

#[test]
fn accepts_valid_distinct_tokens_and_defaults_port_and_static_dir() {
    let config = AppConfig::from_values(
        Some("bearer".into()),
        Some("probe".into()),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(config.bearer_token, "bearer");
    assert_eq!(config.probe_token, "probe");
    assert_eq!(config.port, 8080);
    assert_eq!(config.static_dir, "frontend/dist");
    assert_eq!(config.assets_dir, "backend/assets");
    assert!(!config.use_fake_combine_backend);
    assert!(config.tls.is_none());
    assert!(config.group.is_none());
}

#[test]
fn enables_fake_combine_backend_only_when_requested() {
    let config = AppConfig::from_values(
        Some("bearer".into()),
        Some("probe".into()),
        None,
        None,
        None,
        Some("fake".into()),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(config.use_fake_combine_backend);
}

#[test]
fn rejects_tls_cert_path_missing() {
    let err = AppConfig::from_values(
        Some("bearer".into()),
        Some("probe".into()),
        None,
        None,
        None,
        None,
        None,
        Some("/path/to/key".into()),
        None,
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(err, ConfigError::TlsCertPathMissing);
}

#[test]
fn rejects_tls_key_path_missing() {
    let err = AppConfig::from_values(
        Some("bearer".into()),
        Some("probe".into()),
        None,
        None,
        None,
        None,
        Some("/path/to/cert".into()),
        None,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(err, ConfigError::TlsKeyPathMissing);
}
