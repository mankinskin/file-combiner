use base64::Engine;

/// Must match `backend::contract::MAX_FILE_BYTES`.
pub(crate) const MAX_FILE_BYTES: u64 = 1_048_576;

#[derive(Clone, PartialEq, serde::Deserialize)]
pub(crate) struct DiagnosticEventView {
    pub(crate) kind: String,
    pub(crate) level: String,
    pub(crate) timestamp_ms: u64,
    pub(crate) message: String,
}

#[derive(Clone, PartialEq, serde::Deserialize)]
pub(crate) struct DiagnosticsView {
    pub(crate) events: Vec<DiagnosticEventView>,
    pub(crate) truncated: bool,
}

#[derive(Clone, PartialEq, Default)]
pub(crate) struct SourceState {
    pub(crate) name: Option<String>,
    pub(crate) content: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CredentialMode {
    SharedKey,
    Group,
    DirectPat,
}

#[derive(Clone, PartialEq)]
pub(crate) enum Credential {
    SharedKey(String),
    Group { name: String, password: String },
    DirectPat(String),
}

impl Credential {
    pub(crate) fn is_complete(&self) -> bool {
        match self {
            Self::SharedKey(key) | Self::DirectPat(key) => !key.is_empty(),
            Self::Group { name, password } => !name.is_empty() && !password.is_empty(),
        }
    }

    pub(crate) fn authorization_header(&self) -> String {
        match self {
            Self::SharedKey(key) | Self::DirectPat(key) => format!("Bearer {key}"),
            Self::Group { name, password } => {
                let encoded =
                    base64::engine::general_purpose::STANDARD.encode(format!("{name}:{password}"));
                format!("Group {encoded}")
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    AccessValidation,
    Combining,
    PreparingDownload,
}

impl Stage {
    pub(crate) const ALL: [Self; 3] = [
        Self::AccessValidation,
        Self::Combining,
        Self::PreparingDownload,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::AccessValidation => "Access validation",
            Self::Combining => "Combining",
            Self::PreparingDownload => "Preparing download",
        }
    }

    pub(crate) fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|stage| *stage == self)
            .expect("stage is in ALL")
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct CombineResult {
    pub(crate) preview: String,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Clone, PartialEq)]
pub(crate) enum RequestState {
    Idle,
    Submitting(Stage),
    Success(CombineResult),
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::Credential;

    #[test]
    fn formats_credential_authorization_headers() {
        assert_eq!(
            Credential::SharedKey("shared".to_string()).authorization_header(),
            "Bearer shared"
        );
        assert_eq!(
            Credential::Group {
                name: "sales".to_string(),
                password: "secret".to_string(),
            }
            .authorization_header(),
            "Group c2FsZXM6c2VjcmV0"
        );
    }

    #[test]
    fn rejects_incomplete_credentials() {
        assert!(!Credential::SharedKey(String::new()).is_complete());
        assert!(!Credential::Group {
            name: "sales".to_string(),
            password: String::new(),
        }
        .is_complete());
    }
}
