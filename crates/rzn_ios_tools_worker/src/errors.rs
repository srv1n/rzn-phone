use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolErrorCode {
    NoSession,
    DeviceLocked,
    InvalidParams,
    ElementNotFound,
    AmbiguousMatch,
    Timeout,
    ActionFailed,
    CommitRequired,
    NotSupported,
    Internal,
}

impl ToolErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolErrorCode::NoSession => "NO_SESSION",
            ToolErrorCode::DeviceLocked => "DEVICE_LOCKED",
            ToolErrorCode::InvalidParams => "INVALID_PARAMS",
            ToolErrorCode::ElementNotFound => "ELEMENT_NOT_FOUND",
            ToolErrorCode::AmbiguousMatch => "AMBIGUOUS_MATCH",
            ToolErrorCode::Timeout => "TIMEOUT",
            ToolErrorCode::ActionFailed => "ACTION_FAILED",
            ToolErrorCode::CommitRequired => "COMMIT_REQUIRED",
            ToolErrorCode::NotSupported => "NOT_SUPPORTED",
            ToolErrorCode::Internal => "INTERNAL",
        }
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ToolCallError {
    pub code: ToolErrorCode,
    pub message: String,
    pub details: Value,
}

impl ToolCallError {
    pub fn new(code: ToolErrorCode, message: impl Into<String>, details: Value) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }
}

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("invalid params: {0}")]
    InvalidParams(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("runtime error: {0}")]
    Runtime(String),
}

pub type WorkerResult<T> = Result<T, WorkerError>;
