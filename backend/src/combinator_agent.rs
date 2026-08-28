//! The thin `combinator-agent` adapter: builds the combine prompt, exposes
//! the one write-capable tool the model may use, and validates whatever file
//! that tool reports having created. No other tool, path, or fallback is
//! available to the model.
use std::{
    fmt,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use agent_builder::{
    agent::{AgentExecutor, AgentRequest, RigAgentExecutor},
    config::Config as AgentConfig,
    template::AgentTemplate,
};
use serde::Deserialize;

/// Maximum bytes the write tool accepts for a combined-file body.
pub const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

/// The two uploaded files the combinator agent is asked to combine.
pub struct CombineRequest {
    pub first_name: String,
    pub first_contents: String,
    pub second_name: String,
    pub second_contents: String,
}

#[derive(Debug)]
pub enum CombineAgentError {
    /// The model/tool run itself failed.
    Execution(String),
    /// The agent finished without the write tool reporting a created file.
    NoFileCreated,
    /// The tool-reported file failed containment, type, or size validation.
    InvalidOutput(String),
}

impl fmt::Display for CombineAgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CombineAgentError::Execution(message) => write!(f, "agent execution failed: {message}"),
            CombineAgentError::NoFileCreated => {
                write!(f, "agent did not create a combined file through its tool")
            }
            CombineAgentError::InvalidOutput(message) => {
                write!(f, "combined file failed validation: {message}")
            }
        }
    }
}

impl std::error::Error for CombineAgentError {}

pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// Runs the combinator agent in a fresh request-scoped directory and returns
/// the path to its validated created file. Production requests use
/// [`RigCombineBackend`]; tests inject a deterministic stand-in.
pub trait CombineAgentBackend: Send + Sync {
    fn combine<'a>(
        &'a self,
        dir: &'a Path,
        request: &'a CombineRequest,
    ) -> BoxFuture<'a, Result<PathBuf, CombineAgentError>>;
}

fn build_prompt(request: &CombineRequest) -> String {
    format!(
        "Combine the two attached files by writing the first file's contents, \
         a newline, then the second file's contents, using the write tool to \
         create the combined file. Do not summarize or alter the content.\n\n\
         Attached file: {}\n--- BEGIN FILE ---\n{}\n--- END FILE ---\n\n\
         Attached file: {}\n--- BEGIN FILE ---\n{}\n--- END FILE ---",
        request.first_name, request.first_contents, request.second_name, request.second_contents,
    )
}

/// Server-owned handle recording whatever path the write tool created.
#[derive(Clone, Default)]
struct CreatedFile(Arc<Mutex<Option<PathBuf>>>);

impl CreatedFile {
    fn set(&self, path: PathBuf) {
        *self.0.lock().unwrap() = Some(path);
    }

    fn get(&self) -> Option<PathBuf> {
        self.0.lock().unwrap().clone()
    }
}

#[derive(Deserialize)]
struct WriteCombinedFileArgs {
    content: String,
}

/// The only write-capable tool authorized for the combinator agent. It
/// always selects its own filename beneath `dir` and never accepts a
/// model-selected path.
struct WriteCombinedFileTool {
    dir: PathBuf,
    created: CreatedFile,
}

impl WriteCombinedFileTool {
    fn new(dir: PathBuf, created: CreatedFile) -> Self {
        Self { dir, created }
    }

    async fn write(&self, content: &str) -> std::io::Result<String> {
        if content.len() > MAX_OUTPUT_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "combined content exceeds the output size limit",
            ));
        }

        let unique = format!(
            "{}-{:?}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            std::thread::current().id(),
        );
        let filename = format!("combined-{unique}.txt");
        let path = self.dir.join(&filename);

        // Atomic create-new: refuses to overwrite an existing path.
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(content.as_bytes())?;
        drop(file);

        self.created.set(path);
        Ok(filename)
    }
}

impl rig::tool::Tool for WriteCombinedFileTool {
    const NAME: &'static str = "write_combined_file";
    type Args = WriteCombinedFileArgs;
    type Output = String;
    type Error = std::io::Error;

    fn description(&self) -> String {
        "Create the combined output file with the given UTF-8 content. \
         Selects and returns its own filename; does not accept a path."
            .to_owned()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": { "content": { "type": "string" } },
            "required": ["content"],
            "additionalProperties": false,
        })
    }

    async fn call(
        &self,
        _context: &mut rig::tool::ToolContext,
        args: Self::Args,
    ) -> std::result::Result<Self::Output, Self::Error> {
        self.write(&args.content).await
    }
}

/// Validates that `path` is a regular, non-symlink file contained in `dir`
/// and within the output size limit, then returns its canonical form.
fn validate_created_file(dir: &Path, path: &Path) -> Result<PathBuf, CombineAgentError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| CombineAgentError::InvalidOutput(error.to_string()))?;
    if !metadata.is_file() {
        return Err(CombineAgentError::InvalidOutput(
            "created path is not a regular file".to_owned(),
        ));
    }
    if metadata.len() > MAX_OUTPUT_BYTES as u64 {
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

    Ok(canonical_path)
}

/// Production backend: builds a fresh Copilot-backed agent per request,
/// authorizing only the request-scoped write tool on top of the template's
/// statically authorized tools.
pub struct RigCombineBackend {
    config: AgentConfig,
    template: AgentTemplate,
}

impl RigCombineBackend {
    pub fn new(config: AgentConfig, template: AgentTemplate) -> Self {
        Self { config, template }
    }
}

impl CombineAgentBackend for RigCombineBackend {
    fn combine<'a>(
        &'a self,
        dir: &'a Path,
        request: &'a CombineRequest,
    ) -> BoxFuture<'a, Result<PathBuf, CombineAgentError>> {
        Box::pin(async move {
            let created = CreatedFile::default();
            let tool = WriteCombinedFileTool::new(dir.to_path_buf(), created.clone());

            let executor =
                RigAgentExecutor::from_template(&self.config, &self.template, |server| {
                    server.tool(tool)
                })
                .await
                .map_err(|error| CombineAgentError::Execution(error.to_string()))?;

            executor
                .execute(AgentRequest::new(build_prompt(request), 10))
                .await
                .map_err(|error| CombineAgentError::Execution(error.to_string()))?;

            let created_path = created.get().ok_or(CombineAgentError::NoFileCreated)?;
            validate_created_file(dir, &created_path)
        })
    }
}

/// A deterministic backend for backend integration tests: writes the
/// combined file directly through the same write tool and validation path a
/// real model-driven run would use, with no network or credentials.
pub struct FakeCombineBackend;

impl CombineAgentBackend for FakeCombineBackend {
    fn combine<'a>(
        &'a self,
        dir: &'a Path,
        request: &'a CombineRequest,
    ) -> BoxFuture<'a, Result<PathBuf, CombineAgentError>> {
        Box::pin(async move {
            let created = CreatedFile::default();
            let tool = WriteCombinedFileTool::new(dir.to_path_buf(), created.clone());
            let content = format!("{}\n{}", request.first_contents, request.second_contents);
            tool.write(&content)
                .await
                .map_err(|error| CombineAgentError::InvalidOutput(error.to_string()))?;
            let created_path = created.get().ok_or(CombineAgentError::NoFileCreated)?;
            validate_created_file(dir, &created_path)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{
        CombineAgentBackend, CombineRequest, CreatedFile, FakeCombineBackend, WriteCombinedFileTool,
    };

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
    async fn write_tool_creates_bounded_utf8_file_under_dir() {
        let dir = fresh_dir();
        let created = CreatedFile::default();
        let tool = WriteCombinedFileTool::new(dir.clone(), created.clone());

        let filename = tool.write("hello\nworld").await.unwrap();

        let path = created.get().unwrap();
        assert_eq!(path, dir.join(&filename));
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello\nworld");

        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn write_tool_rejects_oversized_content() {
        let dir = fresh_dir();
        let created = CreatedFile::default();
        let tool = WriteCombinedFileTool::new(dir.clone(), created);

        let oversized = "a".repeat(super::MAX_OUTPUT_BYTES + 1);
        let error = tool.write(&oversized).await.unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn fake_backend_produces_validated_combined_file() {
        let dir = fresh_dir();
        let backend = FakeCombineBackend;
        let request = CombineRequest {
            first_name: "a.txt".to_owned(),
            first_contents: "hello".to_owned(),
            second_name: "b.txt".to_owned(),
            second_contents: "world".to_owned(),
        };

        let path = backend.combine(&dir, &request).await.unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "hello\nworld");
        fs::remove_dir_all(dir).unwrap();
    }
}
