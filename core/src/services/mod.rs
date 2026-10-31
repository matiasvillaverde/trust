//! Business services for financial operations
//!
//! This module contains service layer implementations for complex financial operations
//! including profit distribution, fund transfers, account management, and event handling.

/// Advisory services for concentration/risk recommendations.
pub mod advisory;
/// Fund transfer service for handling transfers between accounts
pub mod fund_transfer_service;
/// Trade grading services and helpers
pub mod grading;
/// Level transition policies and orchestration service
pub mod leveling;
/// Profit distribution service for handling account hierarchy and fund transfers
pub mod profit_distribution_service;
/// Realized gains tax reporting
pub mod tax;
/// Wash sale detection and basis adjustment reporting
pub mod wash_sale;

pub use advisory::{
    AdvisoryAlertLevel, AdvisoryHistoryEntry, AdvisoryResult, AdvisoryThresholds,
    PortfolioAdvisoryStatus, TradeProposal,
};
pub use fund_transfer_service::FundTransferService;
pub use profit_distribution_service::ProfitDistributionService;
pub use tax::{
    TaxAccountSummary, TaxHoldingPeriod, TaxRateOverrides, TaxRates, TaxReport, TaxReportError,
    TaxService, TaxTradeLine,
};
pub use wash_sale::{
    WashSaleError, WashSaleMatch, WashSaleReplacementAdjustment, WashSaleReport, WashSaleService,
};
