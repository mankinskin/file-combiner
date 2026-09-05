//! Runtime configuration loaded from the environment. Startup must fail
//! before binding when a required value is missing, empty, or invalid.
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub bearer_token: String,
    pub probe_token: String,
    pub port: u16,
    pub static_dir: String,
    pub assets_dir: String,
    /// When true, use a deterministic combine backend instead of the real
    /// Copilot-backed agent. Set via `COMBINE_AGENT_BACKEND=fake`; intended
    /// only for smoke tests that must not depend on live model credentials.
    pub use_fake_combine_backend: bool,
    /// TLS configuration for HTTPS. None means HTTP-only (dev/testing only).
    pub tls: Option<TlsConfig>,
    /// Group authentication configuration for browser requests.
    pub group: Option<GroupConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupConfig {
    pub group_name: String,
    pub group_password: String,
    pub group_copilot_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    MissingBearerToken,
    MissingProbeToken,
    TokensMustDiffer,
    InvalidPort(String),
    TlsCertPathMissing,
    TlsKeyPathMissing,
    TlsCertPathInvalid(String),
    TlsKeyPathInvalid(String),
    TlsCertNotReadable(String),
    TlsKeyNotReadable(String),
    MissingGroupName,
    MissingGroupPassword,
    MissingGroupCopilotKey,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingBearerToken => {
                write!(f, "COMBINE_BEARER_TOKEN is missing or empty")
            }
            ConfigError::MissingProbeToken => {
                write!(f, "COMBINE_PROBE_TOKEN is missing or empty")
            }
            ConfigError::TokensMustDiffer => {
                write!(
                    f,
                    "COMBINE_BEARER_TOKEN and COMBINE_PROBE_TOKEN must differ"
                )
            }
            ConfigError::InvalidPort(value) => write!(f, "PORT '{value}' is not a valid port"),
            ConfigError::TlsCertPathMissing => {
                write!(
                    f,
                    "COMBINE_TLS_CERT_PATH is required for HTTPS but missing or empty"
                )
            }
            ConfigError::TlsKeyPathMissing => {
                write!(
                    f,
                    "COMBINE_TLS_KEY_PATH is required for HTTPS but missing or empty"
                )
            }
            ConfigError::TlsCertPathInvalid(path) => {
                write!(f, "COMBINE_TLS_CERT_PATH '{path}' does not exist")
            }
            ConfigError::TlsKeyPathInvalid(path) => {
                write!(f, "COMBINE_TLS_KEY_PATH '{path}' does not exist")
            }
            ConfigError::TlsCertNotReadable(path) => {
                write!(f, "COMBINE_TLS_CERT_PATH '{path}' is not readable")
            }
            ConfigError::TlsKeyNotReadable(path) => {
                write!(f, "COMBINE_TLS_KEY_PATH '{path}' is not readable")
            }
            ConfigError::MissingGroupName => {
                write!(f, "COMBINE_GROUP_NAME is required when group authentication is enabled but missing or empty")
            }
            ConfigError::MissingGroupPassword => {
                write!(f, "COMBINE_GROUP_PASSWORD is required when group authentication is enabled but missing or empty")
            }
            ConfigError::MissingGroupCopilotKey => {
                write!(f, "COMBINE_GROUP_COPILOT_KEY is required when group authentication is enabled but missing or empty")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_values(
            env::var("COMBINE_BEARER_TOKEN").ok(),
            env::var("COMBINE_PROBE_TOKEN").ok(),
            env::var("PORT").ok(),
            env::var("STATIC_DIR").ok(),
            env::var("ASSETS_DIR").ok(),
            env::var("COMBINE_AGENT_BACKEND").ok(),
            env::var("COMBINE_TLS_CERT_PATH").ok(),
            env::var("COMBINE_TLS_KEY_PATH").ok(),
            env::var("COMBINE_GROUP_NAME").ok(),
            env::var("COMBINE_GROUP_PASSWORD").ok(),
            env::var("COMBINE_GROUP_COPILOT_KEY").ok(),
        )
    }

    pub fn from_values(
        bearer_token: Option<String>,
        probe_token: Option<String>,
        port: Option<String>,
        static_dir: Option<String>,
        assets_dir: Option<String>,
        agent_backend: Option<String>,
        tls_cert_path: Option<String>,
        tls_key_path: Option<String>,
        group_name: Option<String>,
        group_password: Option<String>,
        group_copilot_key: Option<String>,
    ) -> Result<Self, ConfigError> {
        let bearer_token = bearer_token.unwrap_or_default();
        if bearer_token.is_empty() {
            return Err(ConfigError::MissingBearerToken);
        }
        let probe_token = probe_token.unwrap_or_default();
        if probe_token.is_empty() {
            return Err(ConfigError::MissingProbeToken);
        }
        if bearer_token == probe_token {
            return Err(ConfigError::TokensMustDiffer);
        }
        let port = match port {
            Some(raw) => raw
                .parse::<u16>()
                .map_err(|_| ConfigError::InvalidPort(raw))?,
            None => 8080,
        };
        let static_dir = static_dir.unwrap_or_else(|| "frontend/dist".to_string());
        let assets_dir = assets_dir.unwrap_or_else(|| "backend/assets".to_string());
        let use_fake_combine_backend = agent_backend.as_deref() == Some("fake");

        // Load group authentication configuration if provided
        let group = match (&group_name, &group_password, &group_copilot_key) {
            (Some(name), Some(password), Some(copilot_key)) => {
                if name.is_empty() {
                    return Err(ConfigError::MissingGroupName);
                }
                if password.is_empty() {
                    return Err(ConfigError::MissingGroupPassword);
                }
                if copilot_key.is_empty() {
                    return Err(ConfigError::MissingGroupCopilotKey);
                }
                Some(GroupConfig {
                    group_name: name.clone(),
                    group_password: password.clone(),
                    group_copilot_key: copilot_key.clone(),
                })
            }
            (None, None, None) => None,
            (Some(_), _, _) | (_, Some(_), _) | (_, _, Some(_)) => {
                // If any group credential is provided, all must be provided
                if group_name.is_some() && group_password.is_none() {
                    return Err(ConfigError::MissingGroupPassword);
                }
                if group_name.is_some() && group_copilot_key.is_none() {
                    return Err(ConfigError::MissingGroupCopilotKey);
                }
                if group_password.is_some() && group_name.is_none() {
                    return Err(ConfigError::MissingGroupName);
                }
                if group_password.is_some() && group_copilot_key.is_none() {
                    return Err(ConfigError::MissingGroupCopilotKey);
                }
                if group_copilot_key.is_some() && group_name.is_none() {
                    return Err(ConfigError::MissingGroupName);
                }
                if group_copilot_key.is_some() && group_password.is_none() {
                    return Err(ConfigError::MissingGroupPassword);
                }
                None
            }
        };

        // Load TLS configuration if both cert and key paths are provided
        let tls = match (tls_cert_path, tls_key_path) {
            (Some(cert_path), Some(key_path)) => {
                if cert_path.is_empty() {
                    return Err(ConfigError::TlsCertPathMissing);
                }
                if key_path.is_empty() {
                    return Err(ConfigError::TlsKeyPathMissing);
                }

                let cert_path_buf = PathBuf::from(&cert_path);
                let key_path_buf = PathBuf::from(&key_path);

                // Verify cert file exists and is readable
                if !cert_path_buf.exists() {
                    return Err(ConfigError::TlsCertPathInvalid(cert_path));
                }
                if std::fs::metadata(&cert_path_buf).is_err() {
                    return Err(ConfigError::TlsCertNotReadable(cert_path));
                }

                // Verify key file exists and is readable
                if !key_path_buf.exists() {
                    return Err(ConfigError::TlsKeyPathInvalid(key_path));
                }
                if std::fs::metadata(&key_path_buf).is_err() {
                    return Err(ConfigError::TlsKeyNotReadable(key_path));
                }

                Some(TlsConfig {
                    cert_path: cert_path_buf,
                    key_path: key_path_buf,
                })
            }
            (None, None) => None,
            (Some(_), None) => return Err(ConfigError::TlsKeyPathMissing),
            (None, Some(_)) => return Err(ConfigError::TlsCertPathMissing),
        };

        Ok(AppConfig {
            bearer_token,
            probe_token,
            port,
            static_dir,
            assets_dir,
            use_fake_combine_backend,
            tls,
            group,
        })
    }
}

#[cfg(test)]
mod tests {
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
}
