use thiserror::Error;

/// Error type for advisor crate configuration and stub advisory features.
#[derive(Debug, Error)]
pub enum AdvisorError {
    /// A required secret or configuration value was blank.
    #[error("{field} cannot be blank")]
    BlankValue {
        /// Field name that failed validation.
        field: &'static str,
    },
    /// A calendar provider string could not be parsed.
    #[error("unsupported calendar provider '{value}'")]
    InvalidCalendarProvider {
        /// Raw provider value supplied by the caller.
        value: String,
    },
    /// The system keychain could not be read or written.
    #[error("advisor keychain error: {0}")]
    Keychain(String),
    /// HTTP client setup or execution failed.
    #[error("advisor HTTP error: {0}")]
    Http(String),
    /// The requested advisor feature is intentionally not implemented yet.
    #[error("{feature} advisory is not implemented yet")]
    NotImplemented {
        /// Stub feature name.
        feature: &'static str,
    },
}

impl From<keyring::Error> for AdvisorError {
    fn from(value: keyring::Error) -> Self {
        AdvisorError::Keychain(value.to_string())
    }
}

impl From<reqwest::Error> for AdvisorError {
    fn from(value: reqwest::Error) -> Self {
        AdvisorError::Http(value.to_string())
    }
}
