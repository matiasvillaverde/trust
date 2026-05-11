use chrono::NaiveDate;
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
    /// A catalyst scan window ended before it started.
    #[error("invalid catalyst scan window: {start_date} through {end_date}")]
    InvalidDateWindow {
        /// Inclusive start date supplied by the caller.
        start_date: NaiveDate,
        /// Inclusive end date supplied by the caller.
        end_date: NaiveDate,
    },
    /// Calendar API returned a response that could not be interpreted.
    #[error("calendar API response error: {message}")]
    CalendarResponse {
        /// Human-readable response parsing error.
        message: String,
    },
    /// The system keychain could not be read or written.
    #[error("advisor keychain error: {0}")]
    Keychain(String),
    /// HTTP client setup or execution failed.
    #[error("advisor HTTP error: {0}")]
    Http(String),
    /// Persisting advisor output through model database traits failed.
    #[error("advisor persistence error: {0}")]
    Persistence(String),
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
