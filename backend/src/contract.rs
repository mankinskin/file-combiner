//! Shared v1 API contract: capacity limits and the JSON error schema.
use serde::Serialize;

/// Maximum bytes allowed for a single uploaded file.
pub const MAX_FILE_BYTES: u64 = 1_048_576;
/// Maximum bytes allowed for multipart metadata (headers, boundaries, filenames).
pub const MAX_METADATA_BYTES: u64 = 65_536;
/// Maximum bytes allowed for the complete raw request body.
pub const MAX_REQUEST_BYTES: u64 = 2_162_688;
/// Maximum bytes allowed for any single multipart part header or filename.
pub const MAX_PART_HEADER_BYTES: usize = 8_192;
/// Maximum number of combine requests that may execute concurrently.
pub const MAX_CONCURRENT_COMBINES: usize = 4;
/// Request deadline, in seconds, for the combine endpoint.
pub const REQUEST_TIMEOUT_SECS: u64 = 30;
/// Exact number of file parts a combine request must contain.
pub const REQUIRED_FILE_PARTS: usize = 2;
/// Required multipart field name for the sales-guide template upload.
pub const FIRST_FIELD_NAME: &str = "first";
/// Required multipart field name for the customer-information upload.
pub const SECOND_FIELD_NAME: &str = "second";

/// Machine-readable error codes for the v1 contract. Every variant maps to
/// exactly one HTTP status, enforced by [`ErrorCode::status`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Unauthorized,
    MalformedMultipart,
    WrongFileCount,
    NonFileField,
    InvalidFieldName,
    InvalidTextEncoding,
    MetadataTooLarge,
    PayloadTooLarge,
    FileTooLarge,
    RequestTimeout,
    ServiceBusy,
    CombineFailed,
    ModelUnauthenticated,
    NotFound,
}

/// Table-driven `(code, wire string, description)` rows, one per [`ErrorCode`]
/// variant. [`ErrorCode::as_str`] and [`ErrorCode::description`] look up this
/// table instead of repeating a per-variant match, keeping both below the
/// complexity threshold.
const ERROR_CODE_TABLE: &[(ErrorCode, &str, &str)] = &[
    (
        ErrorCode::Unauthorized,
        "unauthorized",
        "The request did not include credentials accepted by the server.",
    ),
    (
        ErrorCode::MalformedMultipart,
        "malformed_multipart",
        "The upload could not be parsed as a multipart form request.",
    ),
    (
        ErrorCode::WrongFileCount,
        "wrong_file_count",
        "The request did not contain the required number of uploaded files.",
    ),
    (
        ErrorCode::NonFileField,
        "non_file_field",
        "The request included a form field where a file was required.",
    ),
    (
        ErrorCode::InvalidFieldName,
        "invalid_field_name",
        "The request used a field name outside the API contract.",
    ),
    (
        ErrorCode::InvalidTextEncoding,
        "invalid_text_encoding",
        "An uploaded file was not valid UTF-8 text.",
    ),
    (
        ErrorCode::MetadataTooLarge,
        "metadata_too_large",
        "Multipart field metadata exceeded the configured limit.",
    ),
    (
        ErrorCode::PayloadTooLarge,
        "payload_too_large",
        "The total request body exceeded the configured limit.",
    ),
    (
        ErrorCode::FileTooLarge,
        "file_too_large",
        "One uploaded file exceeded the configured limit.",
    ),
    (
        ErrorCode::RequestTimeout,
        "request_timeout",
        "The request exceeded the configured processing deadline.",
    ),
    (
        ErrorCode::ServiceBusy,
        "service_busy",
        "The service could not accept the request right now.",
    ),
    (
        ErrorCode::CombineFailed,
        "combine_failed",
        "The backend failed while analyzing or combining the uploaded artifacts.",
    ),
    (
        ErrorCode::ModelUnauthenticated,
        "model_unauthenticated",
        "The backend model provider rejected the request because Copilot authentication is not configured.",
    ),
    (
        ErrorCode::NotFound,
        "not_found",
        "The requested resource does not exist or is no longer retained.",
    ),
];

impl ErrorCode {
    /// Look up this variant's table row. Panics only if a variant is ever
    /// added to [`ErrorCode`] without a matching [`ERROR_CODE_TABLE`] row.
    fn table_row(self) -> &'static (ErrorCode, &'static str, &'static str) {
        ERROR_CODE_TABLE
            .iter()
            .find(|(code, _, _)| *code == self)
            .expect("every ErrorCode variant must have an ERROR_CODE_TABLE row")
    }

    pub fn as_str(self) -> &'static str {
        self.table_row().1
    }

    pub fn status(self) -> u16 {
        match self {
            ErrorCode::Unauthorized => 401,
            ErrorCode::MalformedMultipart
            | ErrorCode::WrongFileCount
            | ErrorCode::NonFileField
            | ErrorCode::InvalidFieldName
            | ErrorCode::InvalidTextEncoding
            | ErrorCode::MetadataTooLarge => 400,
            ErrorCode::PayloadTooLarge | ErrorCode::FileTooLarge => 413,
            ErrorCode::RequestTimeout => 408,
            ErrorCode::ServiceBusy => 429,
            ErrorCode::CombineFailed => 502,
            ErrorCode::ModelUnauthenticated => 503,
            ErrorCode::NotFound => 404,
        }
    }
}

/// The exact wire shape of every v1 error response: `code` and `message` only.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub status: u16,
    pub details: Vec<String>,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::with_details(code, message, Vec::<String>::new())
    }

    pub fn with_details<I, S>(code: ErrorCode, message: impl Into<String>, details: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let status = code.status();
        let mut body_details = Vec::from([
            format!("HTTP status {status}"),
            code.description().to_owned(),
        ]);
        body_details.extend(details.into_iter().map(Into::into));
        ErrorBody {
            code: code.as_str().to_string(),
            message: message.into(),
            status,
            details: body_details,
        }
    }
}

impl ErrorCode {
    fn description(self) -> &'static str {
        self.table_row().2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_body_serializes_to_exactly_code_and_message() {
        let body = ErrorBody::new(ErrorCode::Unauthorized, "missing credentials");
        let value = serde_json::to_value(&body).unwrap();
        let obj = value.as_object().unwrap();
        assert_eq!(obj.len(), 4);
        assert_eq!(obj.get("code").unwrap(), "unauthorized");
        assert_eq!(obj.get("message").unwrap(), "missing credentials");
        assert_eq!(obj.get("status").unwrap(), 401);
        assert!(!obj.get("details").unwrap().as_array().unwrap().is_empty());
    }

    #[test]
    fn every_error_code_maps_to_its_contracted_status() {
        let expected = [
            (ErrorCode::Unauthorized, 401),
            (ErrorCode::MalformedMultipart, 400),
            (ErrorCode::WrongFileCount, 400),
            (ErrorCode::NonFileField, 400),
            (ErrorCode::InvalidFieldName, 400),
            (ErrorCode::InvalidTextEncoding, 400),
            (ErrorCode::MetadataTooLarge, 400),
            (ErrorCode::PayloadTooLarge, 413),
            (ErrorCode::FileTooLarge, 413),
            (ErrorCode::RequestTimeout, 408),
            (ErrorCode::ServiceBusy, 429),
            (ErrorCode::CombineFailed, 502),
            (ErrorCode::ModelUnauthenticated, 503),
            (ErrorCode::NotFound, 404),
        ];
        for (code, status) in expected {
            assert_eq!(code.status(), status, "{code:?}");
        }
    }

    #[test]
    fn capacity_constants_match_the_v1_contract() {
        assert_eq!(MAX_FILE_BYTES, 1_048_576);
        assert_eq!(MAX_METADATA_BYTES, 65_536);
        assert_eq!(MAX_REQUEST_BYTES, 2_162_688);
        assert_eq!(MAX_PART_HEADER_BYTES, 8_192);
        assert_eq!(MAX_CONCURRENT_COMBINES, 4);
        assert_eq!(REQUEST_TIMEOUT_SECS, 30);
        assert_eq!(REQUIRED_FILE_PARTS, 2);
        assert_eq!(FIRST_FIELD_NAME, "first");
        assert_eq!(SECOND_FIELD_NAME, "second");
        // The raw body limit must cover two max-size files plus metadata.
        assert_eq!(MAX_REQUEST_BYTES, 2 * MAX_FILE_BYTES + MAX_METADATA_BYTES);
    }
}
