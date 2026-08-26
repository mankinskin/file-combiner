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

/// Machine-readable error codes for the v1 contract. Every variant maps to
/// exactly one HTTP status, enforced by [`ErrorCode::status`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Unauthorized,
    MalformedMultipart,
    WrongFileCount,
    NonFileField,
    InvalidTextEncoding,
    MetadataTooLarge,
    PayloadTooLarge,
    FileTooLarge,
    RequestTimeout,
    ServiceBusy,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Unauthorized => "unauthorized",
            ErrorCode::MalformedMultipart => "malformed_multipart",
            ErrorCode::WrongFileCount => "wrong_file_count",
            ErrorCode::NonFileField => "non_file_field",
            ErrorCode::InvalidTextEncoding => "invalid_text_encoding",
            ErrorCode::MetadataTooLarge => "metadata_too_large",
            ErrorCode::PayloadTooLarge => "payload_too_large",
            ErrorCode::FileTooLarge => "file_too_large",
            ErrorCode::RequestTimeout => "request_timeout",
            ErrorCode::ServiceBusy => "service_busy",
        }
    }

    pub fn status(self) -> u16 {
        match self {
            ErrorCode::Unauthorized => 401,
            ErrorCode::MalformedMultipart
            | ErrorCode::WrongFileCount
            | ErrorCode::NonFileField
            | ErrorCode::InvalidTextEncoding
            | ErrorCode::MetadataTooLarge => 400,
            ErrorCode::PayloadTooLarge | ErrorCode::FileTooLarge => 413,
            ErrorCode::RequestTimeout => 408,
            ErrorCode::ServiceBusy => 429,
        }
    }
}

/// The exact wire shape of every v1 error response: `code` and `message` only.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        ErrorBody {
            code: code.as_str().to_string(),
            message: message.into(),
        }
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
        assert_eq!(obj.len(), 2);
        assert_eq!(obj.get("code").unwrap(), "unauthorized");
        assert_eq!(obj.get("message").unwrap(), "missing credentials");
    }

    #[test]
    fn every_error_code_maps_to_its_contracted_status() {
        let expected = [
            (ErrorCode::Unauthorized, 401),
            (ErrorCode::MalformedMultipart, 400),
            (ErrorCode::WrongFileCount, 400),
            (ErrorCode::NonFileField, 400),
            (ErrorCode::InvalidTextEncoding, 400),
            (ErrorCode::MetadataTooLarge, 400),
            (ErrorCode::PayloadTooLarge, 413),
            (ErrorCode::FileTooLarge, 413),
            (ErrorCode::RequestTimeout, 408),
            (ErrorCode::ServiceBusy, 429),
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
        // The raw body limit must cover two max-size files plus metadata.
        assert_eq!(MAX_REQUEST_BYTES, 2 * MAX_FILE_BYTES + MAX_METADATA_BYTES);
    }
}
