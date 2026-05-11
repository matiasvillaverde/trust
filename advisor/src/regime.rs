use crate::AdvisorError;

/// Request for future market-regime filtering.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegimeRequest {
    /// Symbol or market index to evaluate.
    pub symbol: String,
}

/// Result returned by future market-regime filtering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegimeAdvisory {
    /// Symbol or market index that was evaluated.
    pub symbol: String,
    /// Regime label.
    pub regime: String,
    /// Human-readable explanation.
    pub reason: String,
}

/// Stub market-regime filter.
#[derive(Debug, Clone, Default)]
pub struct RegimeFilter;

impl RegimeFilter {
    /// Return a deliberate stub error until regime filtering is implemented.
    pub fn evaluate(&self, _request: &RegimeRequest) -> Result<RegimeAdvisory, AdvisorError> {
        Err(AdvisorError::NotImplemented { feature: "regime" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regime_filter_is_explicit_stub() {
        let request = RegimeRequest {
            symbol: "SPY".to_string(),
        };

        let error = RegimeFilter.evaluate(&request).unwrap_err();

        assert!(error.to_string().contains("regime"));
    }
}
