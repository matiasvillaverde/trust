//! Legacy error command responsibility. Business mutations go through TrustFacade.
use std::fmt::Display;
use std::fmt::Formatter;

#[derive(Debug)]
pub struct CliError {
    pub(in crate::dispatcher) code: &'static str,
    pub(in crate::dispatcher) message: String,
    pub(in crate::dispatcher) printed: bool,
}

impl CliError {
    pub(in crate::dispatcher) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            printed: false,
        }
    }

    pub(in crate::dispatcher) fn new_printed(
        code: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            printed: true,
        }
    }

    pub fn already_printed(&self) -> bool {
        self.printed
    }
}

impl Display for CliError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CliError {}
