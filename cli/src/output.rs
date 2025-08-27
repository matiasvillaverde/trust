//! CLI Output formatting utilities
//!
//! This module provides structured output formatting for CLI commands
//! following KISS principles with simple, testable functions.

use model::{Account, DistributionResult};
use rust_decimal::Decimal;

/// Formats account information in a structured table
#[allow(dead_code)]
pub struct AccountFormatter;

impl AccountFormatter {
    /// Formats a single account as a table row
    #[allow(dead_code)]
    pub fn format_account(account: &Account) -> String {
        format!(
            "│ {} │ {:15} │ {:10} │ {:12} │",
            &account.id.to_string()[..8],
            truncate(&account.name, 15),
            format!("{:?}", account.account_type),
            format!("{:?}", account.environment)
        )
    }

    /// Formats account creation success message
    #[allow(dead_code)]
    pub fn format_creation_success(account: &Account) -> String {
        format!(
            "✅ Account Created Successfully\n\
            ┌─────────────────────────────────────────────────────────┐\n\
            │ ID:          {} \n\
            │ Name:        {}                    \n\
            │ Type:        {:?}                      \n\
            │ Environment: {:?}                       \n\
            │ Parent:      {}                        \n\
            └─────────────────────────────────────────────────────────┘",
            account.id,
            account.name,
            account.account_type,
            account.environment,
            account
                .parent_account_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "None (Root Account)".to_string())
        )
    }
}

/// Formats distribution-related output
pub struct DistributionFormatter;

impl DistributionFormatter {
    /// Formats distribution result as a detailed breakdown
    pub fn format_distribution_result(result: &DistributionResult) -> String {
        format!(
            "💰 Profit Distribution Executed\n\
            ┌─────────────────────────────────────────────────────────┐\n\
            │ Source Account: {}          \n\
            │ Total Profit:   ${}                      \n\
            │                                                         \n\
            │ Distribution Breakdown:                                 \n\
            │ ├─ Earnings:     ${} → Account: {}   \n\
            │ ├─ Tax Reserve:  ${} → Account: {}   \n\
            │ └─ Reinvestment: ${} → Account: {}   \n\
            └─────────────────────────────────────────────────────────┘",
            result.source_account_id,
            result.original_amount,
            result.earnings_amount.unwrap_or_default(),
            "N/A", // earnings account ID not available in result
            result.tax_amount.unwrap_or_default(),
            "N/A", // tax account ID not available in result
            result.reinvestment_amount.unwrap_or_default(),
            "N/A" // reinvestment account ID not available in result
        )
    }

    /// Formats distribution configuration summary
    pub fn format_configuration_summary(
        earnings_pct: Decimal,
        tax_pct: Decimal,
        reinvestment_pct: Decimal,
        minimum: Decimal,
    ) -> String {
        format!(
            "⚙️  Distribution Rules Configured\n\
            ┌─────────────────────────────────────────────────────────┐\n\
            │ Allocation Rules:                                       \n\
            │ ├─ Earnings:     {}% of profit              \n\
            │ ├─ Tax Reserve:  {}% of profit              \n\
            │ └─ Reinvestment: {}% of profit              \n\
            │                                                         \n\
            │ Minimum Threshold: ${}                     \n\
            │ (Only profits above this amount will be distributed)    \n\
            └─────────────────────────────────────────────────────────┘",
            earnings_pct * Decimal::from(100),
            tax_pct * Decimal::from(100),
            reinvestment_pct * Decimal::from(100),
            minimum
        )
    }
}

/// Formats error messages with helpful context
pub struct ErrorFormatter;

impl ErrorFormatter {
    /// Formats validation errors with suggestions
    pub fn format_validation_error(field: &str, issue: &str, suggestion: &str) -> String {
        format!(
            "❌ Validation Error\n\
            ┌─────────────────────────────────────────────────────────┐\n\
            │ Field:      {}                            \n\
            │ Issue:      {}                            \n\
            │ Suggestion: {}                            \n\
            └─────────────────────────────────────────────────────────┘",
            field, issue, suggestion
        )
    }

    /// Formats system errors with recovery guidance
    pub fn format_system_error(error: &str, recovery: &str) -> String {
        format!(
            "🚨 System Error\n\
            ┌─────────────────────────────────────────────────────────┐\n\
            │ Error:    {}                              \n\
            │ Recovery: {}                              \n\
            └─────────────────────────────────────────────────────────┘",
            error, recovery
        )
    }
}

/// Progress indicator for long-running operations
pub struct ProgressIndicator {
    message: String,
    steps: usize,
    current: usize,
}

impl ProgressIndicator {
    /// Creates a new progress indicator
    pub fn new(message: String, total_steps: usize) -> Self {
        Self {
            message,
            steps: total_steps,
            current: 0,
        }
    }

    /// Updates progress and displays current status
    pub fn step(&mut self, step_message: &str) {
        self.current += 1;
        let progress = (self.current as f32 / self.steps as f32 * 100.0) as usize;

        println!(
            "🔄 {} [{}/{}] ({}%) - {}",
            self.message, self.current, self.steps, progress, step_message
        );
    }

    /// Completes the progress indicator
    pub fn complete(&self) {
        println!("✅ {} - Complete!", self.message);
    }
}

/// Helper function to truncate strings to a maximum length
#[allow(dead_code)]
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        format!("{:width$}", s, width = max_len)
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use model::{AccountType, Environment};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn create_test_account() -> Account {
        Account {
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            deleted_at: None,
            name: "Test Account".to_string(),
            description: "Test account for formatting".to_string(),
            environment: Environment::Paper,
            taxes_percentage: dec!(25.0),
            earnings_percentage: dec!(30.0),
            account_type: AccountType::Primary,
            parent_account_id: None,
        }
    }

    #[test]
    fn test_account_formatter_format_account() {
        let account = create_test_account();
        let result = AccountFormatter::format_account(&account);

        assert!(result.contains("550e8400")); // ID prefix
        assert!(result.contains("Test Account"));
        assert!(result.contains("Primary"));
        assert!(result.contains("Paper"));
    }

    #[test]
    fn test_account_formatter_creation_success() {
        let account = create_test_account();
        let result = AccountFormatter::format_creation_success(&account);

        assert!(result.contains("✅ Account Created Successfully"));
        assert!(result.contains(&account.id.to_string()));
        assert!(result.contains("Test Account"));
        assert!(result.contains("Primary"));
        assert!(result.contains("None (Root Account)"));
    }

    #[test]
    fn test_distribution_formatter_configuration_summary() {
        let result = DistributionFormatter::format_configuration_summary(
            dec!(0.40),
            dec!(0.30),
            dec!(0.30),
            dec!(100.0),
        );

        assert!(result.contains("⚙️  Distribution Rules Configured"));
        assert!(result.contains("40% of profit"));
        assert!(result.contains("30% of profit"));
        assert!(result.contains("$100"));
    }

    #[test]
    fn test_error_formatter_validation_error() {
        let result = ErrorFormatter::format_validation_error(
            "Amount",
            "Must be positive",
            "Please enter a value greater than 0",
        );

        assert!(result.contains("❌ Validation Error"));
        assert!(result.contains("Amount"));
        assert!(result.contains("Must be positive"));
        assert!(result.contains("Please enter a value greater than 0"));
    }

    #[test]
    fn test_progress_indicator_creation() {
        let progress = ProgressIndicator::new("Testing".to_string(), 3);
        assert_eq!(progress.current, 0);
        assert_eq!(progress.steps, 3);
        assert_eq!(progress.message, "Testing");
    }

    #[test]
    fn test_truncate_function() {
        assert_eq!(truncate("short", 10), "short     ");
        assert_eq!(truncate("this is a very long string", 10), "this is...");
        assert_eq!(truncate("exactly10c", 10), "exactly10c");
    }
}
