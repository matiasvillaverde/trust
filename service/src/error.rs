use serde::{Deserialize, Serialize};

/// Stable, sanitized application error. Internal provider errors never cross the boundary.
#[derive(Clone, Debug, Serialize, Deserialize, thiserror::Error, schemars::JsonSchema)]
#[error("{message}")]
pub struct ServiceError {
    pub code: String,
    pub message: String,
    pub next_action: String,
    pub retryable: bool,
}

impl ServiceError {
    pub fn new(code: &str, message: impl Into<String>, next_action: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            next_action: next_action.into(),
            retryable: false,
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(
            "invalid_argument",
            message,
            "Read the tool input schema and correct the request.",
        )
    }

    /// Retrying the same intent is safe only when dispatch never started.
    pub fn before_dispatch_retry(code: &str, message: &str, next_action: &str) -> Self {
        Self {
            retryable: true,
            ..Self::new(code, message, next_action)
        }
    }

    pub fn forbidden() -> Self {
        Self::new(
            "forbidden",
            "This identity cannot perform the requested operation.",
            "Ask the account owner for the necessary account and action grant.",
        )
    }

    pub fn internal() -> Self {
        Self::new(
            "service_unavailable",
            "The operation could not be completed safely.",
            "Inspect the operation status before retrying a mutation.",
        )
    }
}

pub type Result<T> = std::result::Result<T, ServiceError>;
