use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("internal error")]
    Internal,
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_input",
            Self::NotFound(_) => "not_found",
            Self::Internal => "internal_error",
        }
    }
}

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorEnvelope {
    pub code: String,
    pub message: String,
    pub details: Vec<String>,
}

pub fn tonic_code_to_http(code: tonic::Code) -> StatusCode {
    match code {
        tonic::Code::InvalidArgument => StatusCode::BAD_REQUEST,
        tonic::Code::Unauthenticated => StatusCode::UNAUTHORIZED,
        tonic::Code::PermissionDenied => StatusCode::FORBIDDEN,
        tonic::Code::NotFound => StatusCode::NOT_FOUND,
        tonic::Code::AlreadyExists => StatusCode::CONFLICT,
        tonic::Code::FailedPrecondition => StatusCode::PRECONDITION_FAILED,
        tonic::Code::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        tonic::Code::DeadlineExceeded => StatusCode::GATEWAY_TIMEOUT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub fn tonic_code_to_error_code(code: tonic::Code) -> &'static str {
    match code {
        tonic::Code::Ok => "ok",
        tonic::Code::Cancelled => "cancelled",
        tonic::Code::Unknown => "unknown",
        tonic::Code::InvalidArgument => "invalid_argument",
        tonic::Code::DeadlineExceeded => "deadline_exceeded",
        tonic::Code::NotFound => "not_found",
        tonic::Code::AlreadyExists => "already_exists",
        tonic::Code::PermissionDenied => "permission_denied",
        tonic::Code::ResourceExhausted => "resource_exhausted",
        tonic::Code::FailedPrecondition => "failed_precondition",
        tonic::Code::Aborted => "aborted",
        tonic::Code::OutOfRange => "out_of_range",
        tonic::Code::Unimplemented => "unimplemented",
        tonic::Code::Internal => "internal",
        tonic::Code::Unavailable => "unavailable",
        tonic::Code::DataLoss => "data_loss",
        tonic::Code::Unauthenticated => "unauthenticated",
    }
}

pub fn envelope_from_tonic_status(status: &tonic::Status) -> ErrorEnvelope {
    ErrorEnvelope {
        code: tonic_code_to_error_code(status.code()).to_string(),
        message: status.message().to_string(),
        details: Vec::new(),
    }
}

pub fn http_error_from_tonic_status(status: &tonic::Status) -> (StatusCode, ErrorEnvelope) {
    (
        tonic_code_to_http(status.code()),
        envelope_from_tonic_status(status),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_error_code_mapping() {
        assert_eq!(AppError::Internal.code(), "internal_error");
    }

    #[test]
    fn maps_tonic_status_to_standard_envelope() {
        let status = tonic::Status::invalid_argument("bad request");
        let (http, envelope) = http_error_from_tonic_status(&status);
        assert_eq!(http, StatusCode::BAD_REQUEST);
        assert_eq!(envelope.code, "invalid_argument");
        assert_eq!(envelope.message, "bad request");
        assert!(envelope.details.is_empty());
    }
}
