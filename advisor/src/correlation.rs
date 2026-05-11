use crate::AdvisorError;

/// Request for future correlation analysis.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CorrelationRequest {
    /// Primary symbol under review.
    pub symbol: String,
    /// Symbols already represented in the account portfolio.
    pub portfolio_symbols: Vec<String>,
}

/// Result returned by future correlation analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrelationAdvisory {
    /// Primary symbol under review.
    pub symbol: String,
    /// Advisory severity label.
    pub level: String,
    /// Human-readable explanation.
    pub reason: String,
}

/// Stub correlation analyzer.
#[derive(Debug, Clone, Default)]
pub struct CorrelationAnalyzer;

impl CorrelationAnalyzer {
    /// Return a deliberate stub error until correlation analysis is implemented.
    pub fn analyze(
        &self,
        _request: &CorrelationRequest,
    ) -> Result<CorrelationAdvisory, AdvisorError> {
        Err(AdvisorError::NotImplemented {
            feature: "correlation",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlation_analyzer_is_explicit_stub() {
        let request = CorrelationRequest {
            symbol: "AAPL".to_string(),
            portfolio_symbols: vec!["MSFT".to_string()],
        };

        let error = CorrelationAnalyzer.analyze(&request).unwrap_err();

        assert!(error.to_string().contains("correlation"));
    }
}
