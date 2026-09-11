//! Bounded multipart parsing: a request-scoped staging directory plus the
//! two-file, size-capped decode used by the combine pipeline.
use std::fs;
use std::path::{Path, PathBuf};

use axum::extract::Multipart;
use axum::response::Response;

use crate::contract::{
    ErrorCode, FIRST_FIELD_NAME, MAX_FILE_BYTES, MAX_PART_HEADER_BYTES, MAX_REQUEST_BYTES,
    SECOND_FIELD_NAME,
};

use super::handler::error_response;

/// A fresh, request-scoped temporary directory. Its contents (staged
/// uploads and any agent-created output) are removed when the guard drops,
/// which covers success, failure, timeout cancellation, and disconnect.
pub(super) struct RequestDir(PathBuf);

impl RequestDir {
    pub(super) fn create() -> std::io::Result<Self> {
        let unique = format!(
            "{}-{:?}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            std::thread::current().id(),
        );
        let dir = std::env::temp_dir().join(format!("file-combiner-{unique}"));
        fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }

    /// The directory's unique basename, reused as the diagnostics
    /// correlation id for this request.
    pub(super) fn id(&self) -> &str {
        self.0
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown-request")
    }
}

impl Drop for RequestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) struct UploadedFile {
    pub(super) name: String,
    pub(super) contents: String,
}

/// Parses the combine multipart body into its two named file fields,
/// enforcing part-metadata, per-file, and running-total byte caps.
/// Returns the running total alongside the files so the caller can log it.
pub(super) async fn parse_combine_multipart(
    multipart: &mut Multipart,
) -> Result<(UploadedFile, UploadedFile, u64), Response> {
    let mut first_file: Option<UploadedFile> = None;
    let mut second_file: Option<UploadedFile> = None;
    let mut total_bytes: u64 = 0;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => {
                return Err(error_response(
                    ErrorCode::MalformedMultipart,
                    "malformed multipart body",
                ))
            }
        };

        let metadata_len = field.name().unwrap_or_default().len()
            + field.file_name().unwrap_or_default().len()
            + field.content_type().unwrap_or_default().len();
        if metadata_len > MAX_PART_HEADER_BYTES {
            return Err(error_response(
                ErrorCode::MetadataTooLarge,
                "part metadata too large",
            ));
        }

        let field_name = field.name().unwrap_or_default().to_owned();

        let Some(file_name) = field.file_name().map(str::to_owned) else {
            return Err(error_response(
                ErrorCode::NonFileField,
                "only file parts are accepted",
            ));
        };

        // Named-field enforcement verifies field names only; callers remain
        // responsible for assigning template and customer content correctly.
        let target = match field_name.as_str() {
            FIRST_FIELD_NAME if first_file.is_none() => &mut first_file,
            SECOND_FIELD_NAME if second_file.is_none() => &mut second_file,
            FIRST_FIELD_NAME | SECOND_FIELD_NAME => {
                return Err(error_response(
                    ErrorCode::InvalidFieldName,
                    "duplicate multipart field name",
                ))
            }
            _ => {
                return Err(error_response(
                    ErrorCode::InvalidFieldName,
                    "file field must be named \"first\" or \"second\"",
                ))
            }
        };

        let mut field = field;
        *target = Some(read_combine_field(&mut field, file_name, &mut total_bytes).await?);
    }

    let (Some(first_file), Some(second_file)) = (first_file, second_file) else {
        return Err(error_response(
            ErrorCode::WrongFileCount,
            "exactly two files are required",
        ));
    };

    Ok((first_file, second_file, total_bytes))
}

/// Reads one named file field's body, enforcing the per-file and
/// running-total request byte caps before decoding it as UTF-8.
async fn read_combine_field(
    field: &mut axum::extract::multipart::Field<'_>,
    file_name: String,
    total_bytes: &mut u64,
) -> Result<UploadedFile, Response> {
    let mut buffer: Vec<u8> = Vec::new();
    loop {
        match field.chunk().await {
            Ok(Some(chunk)) => {
                buffer.extend_from_slice(&chunk);
                if buffer.len() as u64 > MAX_FILE_BYTES {
                    return Err(error_response(
                        ErrorCode::FileTooLarge,
                        "file exceeds the size limit",
                    ));
                }
                *total_bytes += chunk.len() as u64;
                if *total_bytes > MAX_REQUEST_BYTES {
                    return Err(error_response(
                        ErrorCode::PayloadTooLarge,
                        "request body too large",
                    ));
                }
            }
            Ok(None) => break,
            Err(_) => {
                return Err(error_response(
                    ErrorCode::MalformedMultipart,
                    "malformed multipart body",
                ))
            }
        }
    }

    match String::from_utf8(buffer) {
        Ok(contents) => Ok(UploadedFile {
            name: file_name,
            contents,
        }),
        Err(_) => Err(error_response(
            ErrorCode::InvalidTextEncoding,
            "file is not valid UTF-8",
        )),
    }
}
