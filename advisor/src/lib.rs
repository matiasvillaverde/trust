//! AI-powered advisory integration scaffolding for Trust.
//!
//! This crate owns external advisory configuration and HTTP-backed advisory
//! features. Catalyst scanning is implemented against configurable calendar
//! APIs; correlation analysis uses broker bars, and the regime module remains
//! an explicit stub until its issue-specific implementation lands.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::float_arithmetic,
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::too_many_lines
)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![warn(missing_docs, rust_2018_idioms, missing_debug_implementations)]

/// Catalyst-calendar advisory integration.
pub mod catalyst;
/// Keychain-backed advisor configuration.
pub mod config;
/// Correlation advisory integration.
pub mod correlation;
/// Advisor crate error types.
pub mod error;
/// Market-regime advisory scaffolding.
pub mod regime;

pub use catalyst::{CatalystScanRequest, CatalystScanResult, CatalystScanner};
pub use config::{AdvisorConfig, AdvisorConfigUpdate, CalendarCredentials, CalendarProvider};
pub use correlation::{
    CorrelationAdvisory, CorrelationAnalyzer, CorrelationCalculator, CorrelationConfig,
    CorrelationPair, CorrelationRequest, PositionHeat,
};
pub use error::AdvisorError;

/// Entry point for future external advisory integrations.
#[derive(Debug)]
pub struct Advisor {
    config: AdvisorConfig,
    client: reqwest::blocking::Client,
}

impl Advisor {
    /// Build an advisor client from an explicit redacted configuration status.
    pub fn new(config: AdvisorConfig) -> Result<Self, AdvisorError> {
        let client = reqwest::blocking::Client::builder().build()?;
        Ok(Self { config, client })
    }

    /// Build an advisor client from keychain-backed configuration.
    pub fn from_keychain() -> Result<Self, AdvisorError> {
        Self::new(AdvisorConfig::read()?)
    }

    /// Return the redacted configuration currently attached to this advisor.
    pub fn config(&self) -> &AdvisorConfig {
        &self.config
    }

    /// Return the shared HTTP client for future advisory modules.
    pub fn http_client(&self) -> &reqwest::blocking::Client {
        &self.client
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advisor_builds_from_explicit_config() {
        let config = AdvisorConfig {
            calendar_provider: CalendarProvider::Fmp,
            calendar_api_key_configured: true,
            claude_api_key_configured: false,
        };

        let advisor = Advisor::new(config.clone()).unwrap();

        assert_eq!(advisor.config(), &config);
    }
}
