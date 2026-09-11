//! Runtime configuration loaded from the environment. Startup must fail
//! before binding when a required value is missing, empty, or invalid.
use std::env;
use std::path::PathBuf;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

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
            ConfigError::MissingBearerToken
            | ConfigError::MissingProbeToken
            | ConfigError::TokensMustDiffer
            | ConfigError::InvalidPort(_) => self.fmt_token_error(f),
            ConfigError::TlsCertPathMissing
            | ConfigError::TlsKeyPathMissing
            | ConfigError::TlsCertPathInvalid(_)
            | ConfigError::TlsKeyPathInvalid(_)
            | ConfigError::TlsCertNotReadable(_)
            | ConfigError::TlsKeyNotReadable(_) => self.fmt_tls_error(f),
            ConfigError::MissingGroupName
            | ConfigError::MissingGroupPassword
            | ConfigError::MissingGroupCopilotKey => self.fmt_group_error(f),
        }
    }
}

impl ConfigError {
    fn fmt_token_error(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
            _ => unreachable!("fmt_token_error called with a non-token variant"),
        }
    }

    fn fmt_tls_error(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
            _ => unreachable!("fmt_tls_error called with a non-tls variant"),
        }
    }

    fn fmt_group_error(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingGroupName => {
                write!(f, "COMBINE_GROUP_NAME is required when group authentication is enabled but missing or empty")
            }
            ConfigError::MissingGroupPassword => {
                write!(f, "COMBINE_GROUP_PASSWORD is required when group authentication is enabled but missing or empty")
            }
            ConfigError::MissingGroupCopilotKey => {
                write!(f, "COMBINE_GROUP_COPILOT_KEY is required when group authentication is enabled but missing or empty")
            }
            _ => unreachable!("fmt_group_error called with a non-group variant"),
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
        let (bearer_token, probe_token) = Self::parse_tokens(bearer_token, probe_token)?;
        let port = Self::parse_port(port)?;
        let static_dir = static_dir.unwrap_or_else(|| "frontend/dist".to_string());
        let assets_dir = assets_dir.unwrap_or_else(|| "backend/assets".to_string());
        let use_fake_combine_backend = agent_backend.as_deref() == Some("fake");
        let group = Self::parse_group_config(group_name, group_password, group_copilot_key)?;
        let tls = Self::parse_tls_config(tls_cert_path, tls_key_path)?;

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

    fn parse_tokens(
        bearer_token: Option<String>,
        probe_token: Option<String>,
    ) -> Result<(String, String), ConfigError> {
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
        Ok((bearer_token, probe_token))
    }

    fn parse_port(port: Option<String>) -> Result<u16, ConfigError> {
        match port {
            Some(raw) => raw
                .parse::<u16>()
                .map_err(|_| ConfigError::InvalidPort(raw)),
            None => Ok(8080),
        }
    }

    fn parse_group_config(
        group_name: Option<String>,
        group_password: Option<String>,
        group_copilot_key: Option<String>,
    ) -> Result<Option<GroupConfig>, ConfigError> {
        match (&group_name, &group_password, &group_copilot_key) {
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
                Ok(Some(GroupConfig {
                    group_name: name.clone(),
                    group_password: password.clone(),
                    group_copilot_key: copilot_key.clone(),
                }))
            }
            (None, None, None) => Ok(None),
            _ => {
                match Self::partial_group_error(&group_name, &group_password, &group_copilot_key) {
                    Some(err) => Err(err),
                    None => Ok(None),
                }
            }
        }
    }

    /// Reports which required group credential is missing when at least one
    /// of the three fields is present but not all three are.
    fn partial_group_error(
        group_name: &Option<String>,
        group_password: &Option<String>,
        group_copilot_key: &Option<String>,
    ) -> Option<ConfigError> {
        match (
            group_name.is_some(),
            group_password.is_some(),
            group_copilot_key.is_some(),
        ) {
            (false, true, _) => Some(ConfigError::MissingGroupName),
            (true, false, _) => Some(ConfigError::MissingGroupPassword),
            (_, _, false) => Some(ConfigError::MissingGroupCopilotKey),
            _ => None,
        }
    }

    fn parse_tls_config(
        tls_cert_path: Option<String>,
        tls_key_path: Option<String>,
    ) -> Result<Option<TlsConfig>, ConfigError> {
        match (tls_cert_path, tls_key_path) {
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

                Ok(Some(TlsConfig {
                    cert_path: cert_path_buf,
                    key_path: key_path_buf,
                }))
            }
            (None, None) => Ok(None),
            (Some(_), None) => Err(ConfigError::TlsKeyPathMissing),
            (None, Some(_)) => Err(ConfigError::TlsCertPathMissing),
        }
    }
}
