use crate::{Currency, TransactionCategory};
use chrono::{NaiveDateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

/// Distribution rules for profit allocation
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionRules {
    /// Unique identifier for the rules
    pub id: Uuid,
    /// Account ID these rules apply to
    pub account_id: Uuid,
    /// Percentage for earnings allocation (0-1)
    pub earnings_percent: Decimal,
    /// Percentage for tax reserve allocation (0-1)
    pub tax_percent: Decimal,
    /// Percentage for reinvestment allocation (0-1)  
    pub reinvestment_percent: Decimal,
    /// Minimum profit threshold for distribution
    pub minimum_threshold: Decimal,
    /// Hash used to protect rule updates
    pub configuration_password_hash: String,
    /// When the rules were created
    pub created_at: NaiveDateTime,
    /// When the rules were last updated
    pub updated_at: NaiveDateTime,
}

/// Result of executing profit distribution
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionResult {
    /// Source account ID from which distribution originates
    pub source_account_id: Uuid,
    /// Original profit amount being distributed
    pub original_amount: Decimal,
    /// Amount allocated to earnings (optional if percentage is 0)
    pub earnings_amount: Option<Decimal>,
    /// Amount allocated to tax reserve (optional if percentage is 0)
    pub tax_amount: Option<Decimal>,
    /// Amount allocated to reinvestment (optional if percentage is 0)
    pub reinvestment_amount: Option<Decimal>,
    /// Timestamp when distribution was executed
    pub distribution_date: NaiveDateTime,
    /// Transaction IDs created for this distribution
    pub transactions_created: Vec<Uuid>,
}

/// Persisted history record for a distribution execution
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionHistory {
    /// Unique identifier for the history row
    pub id: Uuid,
    /// Account from which profit was distributed
    pub source_account_id: Uuid,
    /// Related trade when distribution came from trade close
    pub trade_id: Option<Uuid>,
    /// Original distributed amount
    pub original_amount: Decimal,
    /// Distribution execution timestamp
    pub distribution_date: NaiveDateTime,
    /// Distributed earnings amount
    pub earnings_amount: Option<Decimal>,
    /// Distributed tax amount
    pub tax_amount: Option<Decimal>,
    /// Distributed reinvestment amount
    pub reinvestment_amount: Option<Decimal>,
    /// Row creation timestamp
    pub created_at: NaiveDateTime,
    /// Row update timestamp
    pub updated_at: NaiveDateTime,
}

/// One leg of a distribution execution (source -> destination).
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionExecutionLeg {
    /// Destination account receiving funds.
    pub to_account_id: Uuid,
    /// Amount to transfer for this leg (must be > 0).
    pub amount: Decimal,
    /// Category used for the source-side withdrawal.
    pub withdrawal_category: TransactionCategory,
    /// Category used for the destination-side deposit.
    pub deposit_category: TransactionCategory,
    /// Optional override IDs used for deterministic testing / fault injection.
    pub forced_withdrawal_tx_id: Option<Uuid>,
    /// Optional override IDs used for deterministic testing / fault injection.
    pub forced_deposit_tx_id: Option<Uuid>,
}

/// A full distribution execution request to be written atomically.
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionExecutionPlan {
    /// Source account from which profit is being distributed.
    pub source_account_id: Uuid,
    /// Currency being transferred.
    pub currency: Currency,
    /// Optional related trade when distribution was triggered from a close.
    pub trade_id: Option<Uuid>,
    /// Original profit amount being distributed (for history/audit).
    pub original_amount: Decimal,
    /// When this distribution was executed.
    pub distribution_date: NaiveDateTime,
    /// Transfer legs to execute (each produces a withdrawal + deposit).
    pub legs: Vec<DistributionExecutionLeg>,
    /// Amounts written to history (for audit/reporting).
    pub earnings_amount: Option<Decimal>,
    /// Amounts written to history (for audit/reporting).
    pub tax_amount: Option<Decimal>,
    /// Amounts written to history (for audit/reporting).
    pub reinvestment_amount: Option<Decimal>,
}

/// Error types for distribution operations
#[derive(Debug, Clone, PartialEq)]
pub enum DistributionError {
    /// Percentages don't sum to 100% (1.0)
    InvalidPercentageSum,
    /// Individual percentage is invalid (negative or > 1.0)
    InvalidPercentage,
    /// Profit amount is below minimum threshold
    BelowMinimumThreshold,
    /// Profit amount is negative or zero
    InvalidProfitAmount,
}

/// Error raised when no distribution rules exist for an account.
#[derive(Debug)]
pub struct DistributionRulesNotFound {
    /// Account missing distribution rules.
    pub account_id: Uuid,
}

impl std::fmt::Display for DistributionRulesNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "No distribution rules configured for account {}",
            self.account_id
        )
    }
}

impl std::error::Error for DistributionRulesNotFound {}

impl std::fmt::Display for DistributionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DistributionError::InvalidPercentageSum => {
                write!(f, "Distribution percentages must sum to 100% or less")
            }
            DistributionError::InvalidPercentage => {
                write!(f, "Individual percentage must be between 0% and 100%")
            }
            DistributionError::BelowMinimumThreshold => {
                write!(f, "Profit amount is below minimum threshold")
            }
            DistributionError::InvalidProfitAmount => write!(f, "Profit amount must be positive"),
        }
    }
}

impl std::error::Error for DistributionError {}

impl DistributionRules {
    /// Creates new distribution rules
    pub fn new(
        account_id: Uuid,
        earnings_percent: Decimal,
        tax_percent: Decimal,
        reinvestment_percent: Decimal,
        minimum_threshold: Decimal,
    ) -> Self {
        let now = Utc::now().naive_utc();
        DistributionRules {
            id: Uuid::new_v4(),
            account_id,
            earnings_percent,
            tax_percent,
            reinvestment_percent,
            minimum_threshold,
            configuration_password_hash: String::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Sets a pre-hashed configuration password to protect updates
    #[must_use]
    pub fn with_password_hash(mut self, configuration_password_hash: String) -> Self {
        self.configuration_password_hash = configuration_password_hash;
        self
    }

    /// Creates default distribution rules for an account
    pub fn default_for_account(account_id: Uuid) -> Self {
        Self::new(
            account_id,
            Decimal::new(30, 2),  // 30%
            Decimal::new(25, 2),  // 25%
            Decimal::new(45, 2),  // 45%
            Decimal::new(500, 0), // $500 minimum
        )
    }

    /// Validates that the distribution rules are correct
    pub fn validate(&self) -> Result<(), DistributionError> {
        // Check for negative percentages
        if self.earnings_percent < Decimal::ZERO
            || self.tax_percent < Decimal::ZERO
            || self.reinvestment_percent < Decimal::ZERO
        {
            return Err(DistributionError::InvalidPercentage);
        }

        // Check for percentages over 100% (1.0)
        if self.earnings_percent > Decimal::ONE
            || self.tax_percent > Decimal::ONE
            || self.reinvestment_percent > Decimal::ONE
        {
            return Err(DistributionError::InvalidPercentage);
        }

        // Check that percentages sum to 100% (1.0)
        let total = self
            .earnings_percent
            .checked_add(self.tax_percent)
            .and_then(|sum| sum.checked_add(self.reinvestment_percent))
            .ok_or(DistributionError::InvalidPercentageSum)?;
        if total != Decimal::ONE {
            return Err(DistributionError::InvalidPercentageSum);
        }

        Ok(())
    }

    /// Calculates distribution amounts for a given profit
    pub fn calculate_distribution(
        &self,
        profit: Decimal,
    ) -> Result<DistributionResult, DistributionError> {
        // Validate profit amount
        if profit <= Decimal::ZERO {
            return Err(DistributionError::InvalidProfitAmount);
        }

        // Check minimum threshold
        if profit < self.minimum_threshold {
            return Err(DistributionError::BelowMinimumThreshold);
        }

        // Calculate distribution amounts
        let earnings_amount = profit
            .checked_mul(self.earnings_percent)
            .ok_or(DistributionError::InvalidPercentage)?;
        let tax_amount = profit
            .checked_mul(self.tax_percent)
            .ok_or(DistributionError::InvalidPercentage)?;
        let reinvestment_amount = profit
            .checked_mul(self.reinvestment_percent)
            .ok_or(DistributionError::InvalidPercentage)?;

        Ok(DistributionResult {
            source_account_id: self.account_id,
            original_amount: profit,
            earnings_amount: if earnings_amount > Decimal::ZERO {
                Some(earnings_amount)
            } else {
                None
            },
            tax_amount: if tax_amount > Decimal::ZERO {
                Some(tax_amount)
            } else {
                None
            },
            reinvestment_amount: if reinvestment_amount > Decimal::ZERO {
                Some(reinvestment_amount)
            } else {
                None
            },
            distribution_date: Utc::now().naive_utc(),
            transactions_created: Vec::new(), // No transactions created yet
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distribution_rules_validation_valid() {
        let rules = DistributionRules::new(
            Uuid::new_v4(),
            Decimal::new(30, 2),  // 0.30
            Decimal::new(25, 2),  // 0.25
            Decimal::new(45, 2),  // 0.45
            Decimal::new(500, 0), // $500 minimum
        );

        assert!(rules.validate().is_ok());
    }

    #[test]
    fn test_distribution_rules_validation_invalid_sum() {
        let rules = DistributionRules::new(
            Uuid::new_v4(),
            Decimal::new(30, 2), // 0.30
            Decimal::new(25, 2), // 0.25
            Decimal::new(50, 2), // 0.50 - sums to 105%
            Decimal::new(500, 0),
        );

        assert_eq!(
            rules.validate(),
            Err(DistributionError::InvalidPercentageSum)
        );
    }

    #[test]
    fn test_distribution_rules_validation_negative_percentage() {
        let rules = DistributionRules::new(
            Uuid::new_v4(),
            Decimal::new(-10, 2), // -0.10 (negative)
            Decimal::new(55, 2),  // 0.55
            Decimal::new(45, 2),  // 0.45
            Decimal::new(500, 0),
        );

        assert_eq!(rules.validate(), Err(DistributionError::InvalidPercentage));
    }

    #[test]
    fn test_distribution_rules_validation_percentage_over_100() {
        let rules = DistributionRules::new(
            Uuid::new_v4(),
            Decimal::new(150, 2), // 1.50 (150%)
            Decimal::new(25, 2),  // 0.25
            Decimal::new(25, 2),  // 0.25
            Decimal::new(500, 0),
        );

        assert_eq!(rules.validate(), Err(DistributionError::InvalidPercentage));
    }

    #[test]
    fn test_calculate_distribution_valid() {
        let rules = DistributionRules::new(
            Uuid::new_v4(),
            Decimal::new(30, 2),  // 0.30
            Decimal::new(25, 2),  // 0.25
            Decimal::new(45, 2),  // 0.45
            Decimal::new(500, 0), // $500 minimum
        );

        let profit = Decimal::new(2000, 0); // $2000
        let result = rules.calculate_distribution(profit);

        assert!(result.is_ok());
        let distribution = result.unwrap();

        assert_eq!(distribution.original_amount, Decimal::new(2000, 0));
        assert_eq!(distribution.earnings_amount, Some(Decimal::new(600, 0))); // 30% of $2000
        assert_eq!(distribution.tax_amount, Some(Decimal::new(500, 0))); // 25% of $2000
        assert_eq!(distribution.reinvestment_amount, Some(Decimal::new(900, 0)));
        // 45% of $2000
    }

    #[test]
    fn test_calculate_distribution_below_threshold() {
        let rules = DistributionRules::new(
            Uuid::new_v4(),
            Decimal::new(30, 2),
            Decimal::new(25, 2),
            Decimal::new(45, 2),
            Decimal::new(500, 0), // $500 minimum
        );

        let profit = Decimal::new(300, 0); // $300 (below $500 minimum)
        let result = rules.calculate_distribution(profit);

        assert_eq!(result, Err(DistributionError::BelowMinimumThreshold));
    }

    #[test]
    fn test_calculate_distribution_negative_profit() {
        let rules = DistributionRules::new(
            Uuid::new_v4(),
            Decimal::new(30, 2),
            Decimal::new(25, 2),
            Decimal::new(45, 2),
            Decimal::new(500, 0),
        );

        let profit = Decimal::new(-100, 0); // -$100 (loss)
        let result = rules.calculate_distribution(profit);

        assert_eq!(result, Err(DistributionError::InvalidProfitAmount));
    }

    #[test]
    fn test_calculate_distribution_zero_profit() {
        let rules = DistributionRules::new(
            Uuid::new_v4(),
            Decimal::new(30, 2),
            Decimal::new(25, 2),
            Decimal::new(45, 2),
            Decimal::new(500, 0),
        );

        let profit = Decimal::ZERO;
        let result = rules.calculate_distribution(profit);

        assert_eq!(result, Err(DistributionError::InvalidProfitAmount));
    }

    #[test]
    fn test_distribution_rules_default() {
        let account_id = Uuid::new_v4();
        let rules = DistributionRules::default_for_account(account_id);

        // Default should be valid distribution
        assert!(rules.validate().is_ok());
        assert_eq!(rules.account_id, account_id);

        // Default percentages should sum to 100%
        let total = rules.earnings_percent + rules.tax_percent + rules.reinvestment_percent;
        assert_eq!(total, Decimal::ONE);
    }
}
