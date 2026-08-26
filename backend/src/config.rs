//! Runtime configuration loaded from the environment. Startup must fail
//! before binding when a required value is missing, empty, or invalid.
use std::env;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub bearer_token: String,
    pub probe_token: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    MissingBearerToken,
    MissingProbeToken,
    TokensMustDiffer,
    InvalidPort(String),
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
        )
    }

    pub fn from_values(
        bearer_token: Option<String>,
        probe_token: Option<String>,
        port: Option<String>,
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
        Ok(AppConfig {
            bearer_token,
            probe_token,
            port,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_bearer_token() {
        let err = AppConfig::from_values(None, Some("probe".into()), None).unwrap_err();
        assert_eq!(err, ConfigError::MissingBearerToken);
    }

    #[test]
    fn rejects_empty_bearer_token() {
        let err =
            AppConfig::from_values(Some(String::new()), Some("probe".into()), None).unwrap_err();
        assert_eq!(err, ConfigError::MissingBearerToken);
    }

    #[test]
    fn rejects_missing_probe_token() {
        let err = AppConfig::from_values(Some("bearer".into()), None, None).unwrap_err();
        assert_eq!(err, ConfigError::MissingProbeToken);
    }

    #[test]
    fn rejects_equal_tokens() {
        let err =
            AppConfig::from_values(Some("same".into()), Some("same".into()), None).unwrap_err();
        assert_eq!(err, ConfigError::TokensMustDiffer);
    }

    #[test]
    fn rejects_invalid_port() {
        let err = AppConfig::from_values(
            Some("bearer".into()),
            Some("probe".into()),
            Some("not-a-port".into()),
        )
        .unwrap_err();
        assert!(matches!(err, ConfigError::InvalidPort(_)));
    }

    #[test]
    fn accepts_valid_distinct_tokens_and_defaults_port() {
        let config =
            AppConfig::from_values(Some("bearer".into()), Some("probe".into()), None).unwrap();
        assert_eq!(config.bearer_token, "bearer");
        assert_eq!(config.probe_token, "probe");
        assert_eq!(config.port, 8080);
    }
}
