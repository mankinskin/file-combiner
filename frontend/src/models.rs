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

impl DiagnosticEventView {
    pub(crate) fn local(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            level: "info".to_owned(),
            timestamp_ms: 0,
            message: message.into(),
        }
    }
}

#[derive(Clone, PartialEq, serde::Deserialize)]
pub(crate) struct DiagnosticsView {
    pub(crate) events: Vec<DiagnosticEventView>,
    pub(crate) truncated: bool,
}

impl DiagnosticsView {
    pub(crate) fn empty() -> Self {
        Self {
            events: Vec::new(),
            truncated: false,
        }
    }
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
    AnalyzingTemplate,
    AnalyzingProfile,
    Combining,
    Generating,
}

impl Stage {
    pub(crate) const ALL: [Self; 5] = [
        Self::AccessValidation,
        Self::AnalyzingTemplate,
        Self::AnalyzingProfile,
        Self::Combining,
        Self::Generating,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::AccessValidation => "Access validation",
            Self::AnalyzingTemplate => "Analyzing conversation template",
            Self::AnalyzingProfile => "Analyzing person information",
            Self::Combining => "Combining representations",
            Self::Generating => "Generating artifacts",
        }
    }

    pub(crate) fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|stage| *stage == self)
            .expect("stage is in ALL")
    }
}

#[derive(Clone, PartialEq, serde::Deserialize)]
pub(crate) struct GeneratedArtifact {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) format: String,
    pub(crate) download_url: String,
}

#[derive(Clone, PartialEq, serde::Deserialize)]
pub(crate) struct ArtifactList {
    pub(crate) artifacts: Vec<GeneratedArtifact>,
}

/// One generated artifact together with the text the workbench renders in
/// place. `content` stays `None` while the fetch is in flight and becomes
/// `Err` when it fails, so a failed block never hides the artifacts beside it.
#[derive(Clone, PartialEq)]
pub(crate) struct WorkbenchArtifact {
    pub(crate) artifact: GeneratedArtifact,
    pub(crate) content: Option<Result<String, String>>,
}

impl WorkbenchArtifact {
    pub(crate) fn pending(artifact: GeneratedArtifact) -> Self {
        Self {
            artifact,
            content: None,
        }
    }

    pub(crate) fn file_name(&self) -> String {
        let extension = if self.artifact.format == "markdown" {
            "md"
        } else {
            self.artifact.format.as_str()
        };
        format!("{}.{}", self.artifact.name, extension)
    }
}

#[derive(Clone, PartialEq, serde::Deserialize)]
pub(crate) struct LatentStage {
    pub(crate) name: String,
    pub(crate) purpose: Option<String>,
    pub(crate) questions: Vec<String>,
    pub(crate) information: Vec<String>,
    pub(crate) target_aspects: Vec<String>,
}

#[derive(Clone, PartialEq, serde::Deserialize)]
pub(crate) struct LatentCustomer {
    pub(crate) name: Option<String>,
    pub(crate) situation: Vec<String>,
    pub(crate) goals: Vec<String>,
    pub(crate) constraints: Vec<String>,
    pub(crate) known_information: Vec<String>,
    pub(crate) customer_type: Option<String>,
}

#[derive(Clone, PartialEq, serde::Deserialize)]
pub(crate) struct LatentRepresentation {
    pub(crate) schema_version: u8,
    pub(crate) source_kind: String,
    pub(crate) title: String,
    pub(crate) domain: Option<String>,
    pub(crate) stages: Vec<LatentStage>,
    pub(crate) customer: LatentCustomer,
    pub(crate) motivations: Vec<String>,
    pub(crate) questions: Vec<String>,
    pub(crate) objections: Vec<String>,
    pub(crate) tone: Vec<String>,
}

#[derive(Clone, PartialEq, serde::Deserialize)]
pub(crate) struct AnalysisResponse {
    pub(crate) session_id: String,
    pub(crate) complete: bool,
    pub(crate) representations: Vec<LatentRepresentation>,
    pub(crate) artifacts: Vec<GeneratedArtifact>,
}

#[derive(Clone, PartialEq, serde::Deserialize)]
pub(crate) struct ApiErrorBody {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) status: Option<u16>,
    #[serde(default)]
    pub(crate) details: Vec<String>,
}

impl ApiErrorBody {
    pub(crate) fn local(message: impl Into<String>) -> Self {
        Self {
            code: "client_error".to_owned(),
            message: message.into(),
            status: None,
            details: vec!["The browser could not complete or parse the request.".to_owned()],
        }
    }

    pub(crate) fn from_response(
        status: u16,
        message: impl Into<String>,
        details: Vec<String>,
    ) -> Self {
        Self {
            code: "unexpected_response".to_owned(),
            message: message.into(),
            status: Some(status),
            details,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct FailureDetail {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) status: Option<u16>,
    pub(crate) details: Vec<String>,
    pub(crate) trace: Vec<String>,
}

#[derive(Clone, PartialEq)]
pub(crate) enum RequestState {
    Idle,
    Submitting(Stage),
    Success(ArtifactList),
    Failed(FailureDetail),
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
