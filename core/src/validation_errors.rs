//! Typed domain validation errors exposed by the facade.
pub use crate::validators::funding::{FundValidationError, FundValidationErrorCode};
pub use crate::validators::rule::RuleValidationError;
pub use crate::validators::trade::{TradeValidationError, TradeValidationErrorCode};
pub use crate::validators::transaction::TransactionValidationError;
