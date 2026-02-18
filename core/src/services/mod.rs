//! Business services for financial operations
//!
//! This module contains service layer implementations for complex financial operations
//! including profit distribution, fund transfers, account management, and event handling.

/// Event distribution service for automatic profit distribution on trade closure
pub mod event_distribution_service;
/// Fund transfer service for handling transfers between accounts
pub mod fund_transfer_service;
/// Trade grading services and helpers
pub mod grading;
/// Level transition policies and orchestration service
pub mod leveling;
/// Profit distribution service for handling account hierarchy and fund transfers
pub mod profit_distribution_service;
#[cfg(test)]
pub(crate) mod test_helpers;

pub use event_distribution_service::EventDistributionService;
pub use fund_transfer_service::FundTransferService;
pub use profit_distribution_service::ProfitDistributionService;
