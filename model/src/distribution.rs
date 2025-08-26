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
    /// When the rules were created
    pub created_at: NaiveDateTime,
    /// When the rules were last updated
    pub updated_at: NaiveDateTime,
}

/// Result of executing profit distribution
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionResult {
    /// Original profit amount being distributed
    pub original_profit: Decimal,
    /// Amount allocated to earnings
    pub earnings_amount: Decimal,
    /// Amount allocated to tax reserve
    pub tax_amount: Decimal,
    /// Amount allocated to reinvestment
    pub reinvestment_amount: Decimal,
    /// Transaction IDs created for this distribution
    pub transactions_created: Vec<Uuid>,
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
            created_at: now,
            updated_at: now,
        }
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
        let total = self.earnings_percent + self.tax_percent + self.reinvestment_percent;
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
        let earnings_amount = profit * self.earnings_percent;
        let tax_amount = profit * self.tax_percent;
        let reinvestment_amount = profit * self.reinvestment_percent;

        Ok(DistributionResult {
            original_profit: profit,
            earnings_amount,
            tax_amount,
            reinvestment_amount,
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

        assert_eq!(distribution.original_profit, Decimal::new(2000, 0));
        assert_eq!(distribution.earnings_amount, Decimal::new(600, 0)); // 30% of $2000
        assert_eq!(distribution.tax_amount, Decimal::new(500, 0)); // 25% of $2000
        assert_eq!(distribution.reinvestment_amount, Decimal::new(900, 0)); // 45% of $2000
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
