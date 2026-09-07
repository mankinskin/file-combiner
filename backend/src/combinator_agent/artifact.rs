//! Artifact creation and validation: the server-owned handle recording
//! artifacts the write tool created, the sole write-capable tool authorized
//! for the combinator agent, and the containment/size/type validation
//! applied to whatever that tool reports having created.
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::Deserialize;

use super::template_contract::Artifact;
use super::{CombineAgentError, MAX_ARTIFACT_BYTES, MAX_OUTPUT_BYTES};

/// A server-validated artifact created for one combine request.
#[derive(Debug, Clone)]
pub struct GeneratedArtifact {
    pub id: String,
    pub name: String,
    pub format: String,
    pub path: PathBuf,
}

/// Server-owned handle recording the artifact files the write tool created.
#[derive(Clone, Default)]
pub(super) struct CreatedArtifacts(Arc<Mutex<Vec<GeneratedArtifact>>>);

impl CreatedArtifacts {
    pub(super) fn push(&self, artifact: GeneratedArtifact) {
        self.0.lock().unwrap().push(artifact);
    }

    pub(super) fn get(&self) -> Vec<GeneratedArtifact> {
        self.0.lock().unwrap().clone()
    }
}

#[derive(Deserialize)]
pub(super) struct WriteArtifactArgs {
    name: String,
    content: String,
}

/// The only write-capable tool authorized for the combinator agent. It only
/// creates artifacts declared by the validated template and always selects
/// server-owned IDs and filenames beneath `dir`.
pub(super) struct WriteArtifactTool {
    dir: PathBuf,
    expected: Vec<Artifact>,
    created: CreatedArtifacts,
}

impl WriteArtifactTool {
    pub(super) fn new(dir: PathBuf, expected: Vec<Artifact>, created: CreatedArtifacts) -> Self {
        Self {
            dir,
            expected,
            created,
        }
    }

    pub(super) async fn write(&self, name: &str, content: &str) -> std::io::Result<String> {
        if content.len() > MAX_ARTIFACT_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "artifact content exceeds the per-artifact output size limit",
            ));
        }
        let existing = self.created.get();
        if existing.iter().any(|artifact| artifact.name == name) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "artifact has already been created",
            ));
        }
        let existing_bytes = existing.iter().try_fold(0usize, |total, artifact| {
            fs::metadata(&artifact.path).map(|metadata| total + metadata.len() as usize)
        })?;
        if existing_bytes + content.len() > MAX_OUTPUT_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "combined artifact output exceeds the total size limit",
            ));
        }
        let Some((index, expected)) = self
            .expected
            .iter()
            .enumerate()
            .find(|(_, expected)| expected.name == name)
        else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "artifact name is not declared by the template contract",
            ));
        };
        let id = format!("artifact-{index}");
        let filename = format!("{id}.txt");
        let path = self.dir.join(&filename);

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(content.as_bytes())?;
        drop(file);

        self.created.push(GeneratedArtifact {
            id: id.clone(),
            name: expected.name.clone(),
            format: expected.format.clone(),
            path,
        });
        Ok(id)
    }
}

impl rig::tool::Tool for WriteArtifactTool {
    const NAME: &'static str = "write_artifact";
    type Args = WriteArtifactArgs;
    type Output = String;
    type Error = std::io::Error;

    fn description(&self) -> String {
        "Create one contract-declared artifact with the given name and UTF-8 content. \
         Selects and returns its own identifier and does not accept a path."
            .to_owned()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["name", "content"],
            "additionalProperties": false,
        })
    }

    async fn call(
        &self,
        _context: &mut rig::tool::ToolContext,
        args: Self::Args,
    ) -> std::result::Result<Self::Output, Self::Error> {
        self.write(&args.name, &args.content).await
    }
}

/// Validates that `path` is a regular, non-symlink file contained in `dir`
/// and within the output size limit, then returns its canonical form.
pub(super) fn validate_created_artifact(
    dir: &Path,
    artifact: GeneratedArtifact,
) -> Result<GeneratedArtifact, CombineAgentError> {
    let path = &artifact.path;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| CombineAgentError::InvalidOutput(error.to_string()))?;
    if !metadata.is_file() {
        return Err(CombineAgentError::InvalidOutput(
            "created path is not a regular file".to_owned(),
        ));
    }
    if metadata.len() > MAX_ARTIFACT_BYTES as u64 {
        return Err(CombineAgentError::InvalidOutput(
            "created file exceeds the output size limit".to_owned(),
        ));
    }

    let canonical_dir = dir
        .canonicalize()
        .map_err(|error| CombineAgentError::InvalidOutput(error.to_string()))?;
    let canonical_path = path
        .canonicalize()
        .map_err(|error| CombineAgentError::InvalidOutput(error.to_string()))?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err(CombineAgentError::InvalidOutput(
            "created file escapes the request directory".to_owned(),
        ));
    }

    fs::read_to_string(&canonical_path).map_err(|_| {
        CombineAgentError::InvalidOutput("created file is not valid UTF-8".to_owned())
    })?;

    Ok(GeneratedArtifact {
        path: canonical_path,
        ..artifact
    })
}

pub(super) fn validate_created_artifacts(
    dir: &Path,
    expected_count: usize,
    artifacts: Vec<GeneratedArtifact>,
) -> Result<Vec<GeneratedArtifact>, CombineAgentError> {
    if artifacts.len() != expected_count {
        return Err(CombineAgentError::NoFileCreated);
    }

    let mut total_bytes = 0usize;
    let mut validated = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        let artifact = validate_created_artifact(dir, artifact)?;
        total_bytes += fs::metadata(&artifact.path)
            .map_err(|error| CombineAgentError::InvalidOutput(error.to_string()))?
            .len() as usize;
        validated.push(artifact);
    }
    if total_bytes > MAX_OUTPUT_BYTES {
        return Err(CombineAgentError::InvalidOutput(
            "combined artifact output exceeds the total size limit".to_owned(),
        ));
    }

    validated.sort_by(|left, right| left.id.cmp(&right.id));
    for (index, artifact) in validated.iter().enumerate() {
        if artifact.id != format!("artifact-{index}") {
            return Err(CombineAgentError::InvalidOutput(
                "artifact manifest contains duplicate or undeclared output identifiers".to_owned(),
            ));
        }
    }
    Ok(validated)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{Artifact, CreatedArtifacts, WriteArtifactTool};

    fn fresh_dir() -> std::path::PathBuf {
        let unique = format!(
            "{}-{:?}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::thread::current().id(),
        );
        let dir = std::env::temp_dir().join(format!("combinator-agent-test-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn write_tool_creates_declared_bounded_utf8_artifact_under_dir() {
        let dir = fresh_dir();
        let created = CreatedArtifacts::default();
        let tool = WriteArtifactTool::new(
            dir.clone(),
            vec![Artifact {
                name: "brief".to_owned(),
                format: "markdown".to_owned(),
                sections: vec!["Priorities".to_owned()],
            }],
            created.clone(),
        );

        let id = tool.write("brief", "hello\nworld").await.unwrap();

        let artifacts = created.get();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(id, "artifact-0");
        assert_eq!(artifacts[0].path, dir.join("artifact-0.txt"));
        assert_eq!(
            fs::read_to_string(&artifacts[0].path).unwrap(),
            "hello\nworld"
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn write_tool_accepts_content_exactly_at_per_artifact_limit() {
        let dir = fresh_dir();
        let created = CreatedArtifacts::default();
        let tool = WriteArtifactTool::new(
            dir.clone(),
            vec![Artifact {
                name: "brief".to_owned(),
                format: "markdown".to_owned(),
                sections: vec!["Priorities".to_owned()],
            }],
            created,
        );

        let at_limit = "a".repeat(super::MAX_ARTIFACT_BYTES);
        tool.write("brief", &at_limit).await.unwrap();

        assert_eq!(
            fs::metadata(dir.join("artifact-0.txt")).unwrap().len(),
            super::MAX_ARTIFACT_BYTES as u64
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn write_tool_rejects_undeclared_name_and_oversized_content() {
        let dir = fresh_dir();
        let created = CreatedArtifacts::default();
        let tool = WriteArtifactTool::new(
            dir.clone(),
            vec![Artifact {
                name: "brief".to_owned(),
                format: "markdown".to_owned(),
                sections: vec!["Priorities".to_owned()],
            }],
            created,
        );

        let error = tool.write("unknown", "text").await.unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);

        let oversized = "a".repeat(super::MAX_ARTIFACT_BYTES + 1);
        let error = tool.write("brief", &oversized).await.unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn write_tool_rejects_duplicate_artifact_names() {
        let dir = fresh_dir();
        let created = CreatedArtifacts::default();
        let tool = WriteArtifactTool::new(
            dir.clone(),
            vec![Artifact {
                name: "brief".to_owned(),
                format: "markdown".to_owned(),
                sections: vec!["Priorities".to_owned()],
            }],
            created,
        );

        tool.write("brief", "first").await.unwrap();
        let error = tool.write("brief", "second").await.unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        fs::remove_dir_all(dir).unwrap();
    }
}
