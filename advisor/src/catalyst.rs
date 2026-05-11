use crate::AdvisorError;

/// Request for future calendar catalyst scanning.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalystScanRequest {
    /// Symbols to scan for upcoming events.
    pub symbols: Vec<String>,
}

/// Calendar event returned by future catalyst scanning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalystEvent {
    /// Event symbol.
    pub symbol: String,
    /// Human-readable event type.
    pub event_type: String,
    /// Human-readable event date or timestamp.
    pub event_time: String,
}

/// Stub catalyst scanner.
#[derive(Debug, Clone, Default)]
pub struct CatalystScanner;

impl CatalystScanner {
    /// Return a deliberate stub error until catalyst scanning is implemented.
    pub fn scan(&self, _request: &CatalystScanRequest) -> Result<Vec<CatalystEvent>, AdvisorError> {
        Err(AdvisorError::NotImplemented {
            feature: "catalyst",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalyst_scanner_is_explicit_stub() {
        let request = CatalystScanRequest {
            symbols: vec!["AAPL".to_string()],
        };

        let error = CatalystScanner.scan(&request).unwrap_err();

        assert!(error.to_string().contains("catalyst"));
    }
}
