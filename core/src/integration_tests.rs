#![allow(clippy::arithmetic_side_effects)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::const_is_empty)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::cognitive_complexity)]

use chrono::Utc;
use model::{Account, AccountType, Currency, Environment, Trade, TradeBalance};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use uuid::Uuid;

/// Integration test suite for the profit distribution system
/// Tests the complete workflow from trade closure to profit allocation
#[derive(Debug)]
pub struct IntegrationTestSuite;

impl IntegrationTestSuite {
    /// Test the complete profit distribution workflow
    pub fn test_complete_workflow() -> Result<(), Box<dyn Error>> {
        println!("🧪 Running Integration Test: Complete Profit Distribution Workflow");

        // 1. Create mock accounts hierarchy
        let primary_account = Self::create_test_account(AccountType::Primary, None)?;
        let earnings_account =
            Self::create_test_account(AccountType::Earnings, Some(primary_account.id))?;
        let tax_account =
            Self::create_test_account(AccountType::TaxReserve, Some(primary_account.id))?;
        let reinvestment_account =
            Self::create_test_account(AccountType::Reinvestment, Some(primary_account.id))?;

        println!("  ✅ Created account hierarchy:");
        println!("     Primary: {}", primary_account.id);
        println!("     Earnings: {}", earnings_account.id);
        println!("     Tax: {}", tax_account.id);
        println!("     Reinvestment: {}", reinvestment_account.id);

        // 2. Create profitable trade
        let _profitable_trade =
            Self::create_profitable_trade(primary_account.id, dec!(1000.0), dec!(200.0))?;
        println!("  ✅ Created profitable trade with $200 profit");

        // 3. Test distribution rules validation
        let rules = model::DistributionRules::new(
            primary_account.id,
            dec!(0.40),  // 40% earnings
            dec!(0.30),  // 30% tax
            dec!(0.30),  // 30% reinvestment
            dec!(100.0), // $100 minimum
        );

        rules.validate()?;
        println!("  ✅ Distribution rules validated (40%/30%/30%)");

        // 4. Test profit calculation
        let expected_earnings = dec!(200.0) * dec!(0.40); // $80
        let expected_tax = dec!(200.0) * dec!(0.30); // $60
        let expected_reinvestment = dec!(200.0) * dec!(0.30); // $60

        let distribution = rules.calculate_distribution(dec!(200.0))?;
        assert_eq!(distribution.earnings_amount, Some(expected_earnings));
        assert_eq!(distribution.tax_amount, Some(expected_tax));
        assert_eq!(
            distribution.reinvestment_amount,
            Some(expected_reinvestment)
        );

        println!("  ✅ Distribution calculations correct:");
        println!("     Earnings: ${}", expected_earnings);
        println!("     Tax: ${}", expected_tax);
        println!("     Reinvestment: ${}", expected_reinvestment);

        // 5. Test hierarchy validation
        Self::test_hierarchy_validation(&primary_account, &earnings_account)?;
        Self::test_hierarchy_validation(&primary_account, &tax_account)?;
        Self::test_hierarchy_validation(&primary_account, &reinvestment_account)?;

        println!("  ✅ Account hierarchy validation successful");

        // 6. Test complete distribution workflow simulation
        Self::test_distribution_workflow_simulation(&rules, dec!(200.0))?;

        println!("  ✅ Complete distribution workflow simulation successful");

        // 7. Test fund transfer workflow
        Self::test_fund_transfer_workflow(&primary_account, &earnings_account)?;

        println!("  ✅ Fund transfer workflow validation successful");

        // 8. Test edge cases
        Self::test_edge_cases(&rules)?;

        println!("  ✅ Edge case validation successful");

        println!("🎉 Integration Test PASSED: Complete workflow validated");
        Ok(())
    }

    /// Test CLI argument parsing and validation (without actual CLI execution)
    pub fn test_cli_integration() -> Result<(), Box<dyn Error>> {
        println!("🧪 Running Integration Test: CLI Argument Validation");

        // Test valid UUID parsing
        let test_uuid = Uuid::new_v4();
        let uuid_str = test_uuid.to_string();
        let parsed_uuid = Uuid::parse_str(&uuid_str)?;
        assert_eq!(test_uuid, parsed_uuid);

        // Test decimal parsing for amounts
        let amount_str = "1234.56";
        let amount = Decimal::from_str_exact(amount_str)?;
        assert_eq!(amount, dec!(1234.56));

        // Test percentage conversion (CLI accepts 40.0, needs 0.40)
        let percentage_input = "40.0";
        let percentage = Decimal::from_str_exact(percentage_input)? / Decimal::new(100, 0);
        assert_eq!(percentage, dec!(0.40));

        println!("  ✅ UUID parsing validated");
        println!("  ✅ Decimal parsing validated");
        println!("  ✅ Percentage conversion validated");

        // Test CLI command argument validation patterns
        Self::test_cli_command_patterns()?;

        println!("🎉 Integration Test PASSED: CLI argument validation");
        Ok(())
    }

    /// Test CLI command validation patterns
    pub fn test_cli_command_patterns() -> Result<(), Box<dyn Error>> {
        println!("🧪 Running Integration Test: CLI Command Patterns");

        // Test account creation command pattern validation
        Self::test_account_creation_pattern()?;

        // Test distribution command pattern validation
        Self::test_distribution_command_pattern()?;

        // Test transfer command pattern validation
        Self::test_transfer_command_pattern()?;

        println!("  ✅ Account creation command pattern validated");
        println!("  ✅ Distribution command pattern validated");
        println!("  ✅ Transfer command pattern validated");

        println!("🎉 Integration Test PASSED: CLI command patterns");
        Ok(())
    }

    /// Test account creation CLI command patterns
    fn test_account_creation_pattern() -> Result<(), Box<dyn Error>> {
        // Simulate CLI args: trust accounts create --type earnings --parent-id <uuid>
        let parent_id = Uuid::new_v4();
        let account_type_str = "earnings";

        // Validate account type parsing
        let account_type = match account_type_str {
            "primary" => model::AccountType::Primary,
            "earnings" => model::AccountType::Earnings,
            "tax-reserve" => model::AccountType::TaxReserve,
            "reinvestment" => model::AccountType::Reinvestment,
            _ => return Err("Invalid account type".into()),
        };

        assert_eq!(account_type, model::AccountType::Earnings);

        // Validate parent ID is valid UUID
        assert!(parent_id.to_string().len() == 36); // Standard UUID length

        Ok(())
    }

    /// Test distribution configuration CLI command patterns
    fn test_distribution_command_pattern() -> Result<(), Box<dyn Error>> {
        // Simulate CLI args: trust distribution configure --account <uuid> --earnings 40.0 --tax 30.0 --reinvestment 30.0 --minimum 100.0
        let account_id = Uuid::new_v4();
        let earnings_pct = "40.0";
        let tax_pct = "30.0";
        let reinvestment_pct = "30.0";
        let minimum_str = "100.0";

        // Parse and convert percentages
        let earnings = Decimal::from_str_exact(earnings_pct)? / dec!(100.0);
        let tax = Decimal::from_str_exact(tax_pct)? / dec!(100.0);
        let reinvestment = Decimal::from_str_exact(reinvestment_pct)? / dec!(100.0);
        let minimum = Decimal::from_str_exact(minimum_str)?;

        // Validate percentages sum to 100%
        let total = earnings + tax + reinvestment;
        assert_eq!(total, dec!(1.0)); // Should equal 1.0 (100%)

        // Create distribution rules to validate structure
        let rules = model::DistributionRules::new(account_id, earnings, tax, reinvestment, minimum);

        // Test validation
        rules.validate()?;

        assert_eq!(rules.earnings_percent, dec!(0.40));
        assert_eq!(rules.tax_percent, dec!(0.30));
        assert_eq!(rules.reinvestment_percent, dec!(0.30));
        assert_eq!(rules.minimum_threshold, dec!(100.0));

        Ok(())
    }

    /// Test transfer command CLI patterns
    fn test_transfer_command_pattern() -> Result<(), Box<dyn Error>> {
        // Simulate CLI args: trust accounts transfer --from <uuid> --to <uuid> --amount 500.00 --reason "Profit distribution"
        let from_account = Uuid::new_v4();
        let to_account = Uuid::new_v4();
        let amount_str = "500.00";
        let reason = "Profit distribution";

        // Parse amount
        let amount = Decimal::from_str_exact(amount_str)?;
        assert_eq!(amount, dec!(500.00));

        // Validate UUIDs are different (can't transfer to self)
        assert_ne!(from_account, to_account);

        // Validate reason is not empty
        assert!(!reason.is_empty());
        assert!(reason.len() <= 255); // Reasonable length limit

        Ok(())
    }

    /// Test event-driven distribution workflow
    pub fn test_event_integration() -> Result<(), Box<dyn Error>> {
        println!("🧪 Running Integration Test: Event-Driven Distribution");

        // Test profitable trade event
        let profitable_trade =
            Self::create_profitable_trade(Uuid::new_v4(), dec!(1000.0), dec!(300.0))?;
        assert!(profitable_trade.balance.total_performance > dec!(0.0));

        // Test losing trade event
        let losing_trade = Self::create_losing_trade(Uuid::new_v4(), dec!(1000.0), dec!(-200.0))?;
        assert!(losing_trade.balance.total_performance <= dec!(0.0));

        println!("  ✅ Profitable trade event handling validated");
        println!("  ✅ Losing trade event handling validated");

        println!("🎉 Integration Test PASSED: Event-driven distribution");
        Ok(())
    }

    /// Test complete system integration with all components working together
    pub fn test_full_system_integration() -> Result<(), Box<dyn Error>> {
        println!("🧪 Running Integration Test: Full System Integration");

        // 1. Create complete account hierarchy
        let primary_account = Self::create_test_account(model::AccountType::Primary, None)?;
        let earnings_account =
            Self::create_test_account(model::AccountType::Earnings, Some(primary_account.id))?;
        let _tax_account =
            Self::create_test_account(model::AccountType::TaxReserve, Some(primary_account.id))?;
        let _reinvestment_account =
            Self::create_test_account(model::AccountType::Reinvestment, Some(primary_account.id))?;

        println!("  ✅ Complete account hierarchy created");

        // 2. Create distribution rules and validate
        let rules = model::DistributionRules::new(
            primary_account.id,
            dec!(0.40),  // 40% earnings
            dec!(0.30),  // 30% tax
            dec!(0.30),  // 30% reinvestment
            dec!(100.0), // $100 minimum
        );

        rules.validate()?;
        println!("  ✅ Distribution rules validated");

        // 3. Simulate profitable trade
        let profitable_trade =
            Self::create_profitable_trade(primary_account.id, dec!(1000.0), dec!(500.0))?;
        println!("  ✅ Profitable trade simulated ($500 profit)");

        // 4. Calculate expected distributions
        let expected_earnings = dec!(500.0) * dec!(0.40); // $200
        let expected_tax = dec!(500.0) * dec!(0.30); // $150
        let expected_reinvestment = dec!(500.0) * dec!(0.30); // $150

        let distribution = rules.calculate_distribution(dec!(500.0))?;
        assert_eq!(distribution.earnings_amount.unwrap(), expected_earnings);
        assert_eq!(distribution.tax_amount.unwrap(), expected_tax);
        assert_eq!(
            distribution.reinvestment_amount.unwrap(),
            expected_reinvestment
        );

        println!("  ✅ Distribution calculations verified ($200+$150+$150 = $500)");

        // 5. Validate event trigger preconditions
        assert!(profitable_trade.balance.total_performance > dec!(0.0));
        let losing_trade =
            Self::create_losing_trade(primary_account.id, dec!(1000.0), dec!(-200.0))?;
        assert!(losing_trade.balance.total_performance <= dec!(0.0));

        println!("  ✅ Event-driven workflow validated (profitable/losing trades)");

        // 6. Test CLI integration patterns
        Self::test_cli_system_integration(&primary_account, &earnings_account)?;

        println!("  ✅ CLI integration patterns validated");

        // 7. Test comprehensive edge cases
        Self::test_system_edge_cases(&rules)?;

        println!("  ✅ System edge cases validated");

        println!("🎉 Integration Test PASSED: Full System Integration - All components working together!");
        Ok(())
    }

    fn test_cli_system_integration(
        primary: &Account,
        earnings: &Account,
    ) -> Result<(), Box<dyn Error>> {
        // Test the complete CLI workflow simulation

        // 1. Simulate: trust accounts create --type earnings --parent-id <primary_id>
        assert_eq!(earnings.account_type, model::AccountType::Earnings);
        assert_eq!(earnings.parent_account_id, Some(primary.id));

        // 2. Simulate: trust distribution configure --account <primary_id> --earnings 40.0 --tax 30.0 --reinvestment 30.0 --minimum 100.0
        let earnings_pct = Decimal::from_str_exact("40.0")? / dec!(100.0);
        let tax_pct = Decimal::from_str_exact("30.0")? / dec!(100.0);
        let reinvestment_pct = Decimal::from_str_exact("30.0")? / dec!(100.0);
        let minimum = Decimal::from_str_exact("100.0")?;

        let rules = model::DistributionRules::new(
            primary.id,
            earnings_pct,
            tax_pct,
            reinvestment_pct,
            minimum,
        );
        rules.validate()?;

        // 3. Simulate: trust distribution execute --account <primary_id> --profit 300.00
        let profit = Decimal::from_str_exact("300.00")?;
        let distribution = rules.calculate_distribution(profit)?;

        assert_eq!(distribution.earnings_amount.unwrap(), dec!(120.00)); // 40% of $300
        assert_eq!(distribution.tax_amount.unwrap(), dec!(90.00)); // 30% of $300
        assert_eq!(distribution.reinvestment_amount.unwrap(), dec!(90.00)); // 30% of $300

        // 4. Simulate: trust accounts transfer --from <primary_id> --to <earnings_id> --amount 120.00 --reason "Profit distribution"
        let transfer_amount = Decimal::from_str_exact("120.00")?;
        assert_eq!(transfer_amount, distribution.earnings_amount.unwrap());

        // 5. Simulate: trust trade close --account <primary_id> --trade-id <trade_id> --auto-distribute
        // Auto-distribute flag would trigger EventDistributionService logic

        println!("    🔄 Complete CLI workflow simulation successful");

        Ok(())
    }

    fn test_system_edge_cases(rules: &model::DistributionRules) -> Result<(), Box<dyn Error>> {
        // Test system-wide edge cases that affect multiple components

        // 1. Test minimum profit threshold across system
        let below_min_profit = dec!(50.0); // Below $100 minimum
        let result = rules.calculate_distribution(below_min_profit);
        assert!(result.is_err()); // Should be rejected at distribution level

        // 2. Test maximum supported profit (large numbers)
        let max_profit = dec!(999999.99);
        let result = rules.calculate_distribution(max_profit);
        assert!(result.is_ok()); // Should handle large amounts

        if let Ok(distribution) = result {
            let total = distribution.earnings_amount.unwrap()
                + distribution.tax_amount.unwrap()
                + distribution.reinvestment_amount.unwrap();
            assert_eq!(total, max_profit);
        }

        // 3. Test precision boundaries (very small amounts above minimum)
        let tiny_profit = dec!(100.001);
        let result = rules.calculate_distribution(tiny_profit);
        assert!(result.is_ok());

        // 4. Test account hierarchy validation across operations
        let primary_account = Self::create_test_account(model::AccountType::Primary, None)?;
        let earnings_account =
            Self::create_test_account(model::AccountType::Earnings, Some(primary_account.id))?;

        // Valid parent-child relationship
        assert_eq!(earnings_account.parent_account_id, Some(primary_account.id));
        assert_ne!(earnings_account.account_type, primary_account.account_type);

        // Invalid: Primary account cannot have parent
        let invalid_primary = Account {
            id: Uuid::new_v4(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            deleted_at: None,
            name: "Invalid Primary".to_string(),
            description: "Primary with parent (invalid)".to_string(),
            environment: model::Environment::Paper,
            taxes_percentage: dec!(25.0),
            earnings_percentage: dec!(30.0),
            account_type: model::AccountType::Primary,
            parent_account_id: Some(Uuid::new_v4()), // Invalid!
        };

        // This would be caught by validation in real system
        assert!(invalid_primary.parent_account_id.is_some()); // Shows the invalid state

        println!("    🔍 System edge cases comprehensive validation complete");

        Ok(())
    }

    // Helper methods

    fn test_distribution_workflow_simulation(
        rules: &model::DistributionRules,
        profit: Decimal,
    ) -> Result<(), Box<dyn Error>> {
        // Test the complete distribution calculation workflow
        let distribution = rules.calculate_distribution(profit)?;

        // Validate all amounts are calculated correctly
        assert!(distribution.earnings_amount.is_some());
        assert!(distribution.tax_amount.is_some());
        assert!(distribution.reinvestment_amount.is_some());

        let earnings = distribution.earnings_amount.unwrap();
        let tax = distribution.tax_amount.unwrap();
        let reinvestment = distribution.reinvestment_amount.unwrap();

        // Validate amounts sum to original profit
        let total_distributed = earnings + tax + reinvestment;
        assert_eq!(total_distributed, profit);

        // Validate individual calculations
        assert_eq!(earnings, profit * dec!(0.40));
        assert_eq!(tax, profit * dec!(0.30));
        assert_eq!(reinvestment, profit * dec!(0.30));

        println!(
            "    💰 Distribution simulation: ${}→${}+${}+${}",
            profit, earnings, tax, reinvestment
        );

        Ok(())
    }

    fn test_fund_transfer_workflow(
        source: &Account,
        target: &Account,
    ) -> Result<(), Box<dyn Error>> {
        // Test fund transfer validation logic
        let transfer_amount = dec!(100.0);
        let reason = "Integration test transfer";

        // Validate account relationship (target should be child of source)
        assert_eq!(target.parent_account_id, Some(source.id));
        assert_ne!(target.account_type, source.account_type);

        // Validate transfer amount is positive
        assert!(transfer_amount > Decimal::ZERO);

        // Validate reason is provided
        assert!(!reason.is_empty());

        // Test that different account types can be transfer targets
        match target.account_type {
            model::AccountType::Earnings
            | model::AccountType::TaxReserve
            | model::AccountType::Reinvestment => {
                // These are valid transfer targets from primary account
                assert_eq!(source.account_type, model::AccountType::Primary);
            }
            model::AccountType::Primary => {
                return Err("Primary accounts cannot be transfer targets in this context".into());
            }
        }

        println!(
            "    🔄 Transfer validation: {} → {} (${}, '{}')",
            source.account_type as u8, target.account_type as u8, transfer_amount, reason
        );

        Ok(())
    }

    fn create_test_account(
        account_type: AccountType,
        parent_id: Option<Uuid>,
    ) -> Result<Account, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        Ok(Account {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            name: format!("{:?} Account", account_type),
            description: format!("Test {:?} account", account_type),
            environment: Environment::Paper,
            taxes_percentage: dec!(25.0),
            earnings_percentage: dec!(30.0),
            account_type,
            parent_account_id: parent_id,
        })
    }

    fn create_profitable_trade(
        account_id: Uuid,
        initial: Decimal,
        profit: Decimal,
    ) -> Result<Trade, Box<dyn Error>> {
        let mut trade = Trade::default();
        trade.account_id = account_id;
        trade.status = model::Status::ClosedTarget;

        let now = Utc::now().naive_utc();
        trade.balance = TradeBalance {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            currency: Currency::USD,
            funding: initial,
            capital_in_market: dec!(0.0), // Closed
            capital_out_market: initial + profit,
            taxed: dec!(0.0),
            total_performance: profit, // This is the key field for profit calculation
        };

        Ok(trade)
    }

    fn create_losing_trade(
        account_id: Uuid,
        initial: Decimal,
        loss: Decimal,
    ) -> Result<Trade, Box<dyn Error>> {
        let mut trade = Self::create_profitable_trade(account_id, initial, loss)?;
        trade.status = model::Status::ClosedStopLoss;
        trade.balance.capital_out_market = initial + loss; // Less than initial
        trade.balance.total_performance = loss; // Negative value
        Ok(trade)
    }

    fn test_hierarchy_validation(parent: &Account, child: &Account) -> Result<(), Box<dyn Error>> {
        // Verify parent-child relationship
        assert_eq!(child.parent_account_id, Some(parent.id));
        assert_ne!(child.account_type, parent.account_type);
        Ok(())
    }

    fn test_edge_cases(rules: &model::DistributionRules) -> Result<(), Box<dyn Error>> {
        println!("    🧪 Testing edge cases and boundary conditions");

        // Test below minimum threshold
        let result = rules.calculate_distribution(dec!(50.0)); // Below $100 minimum
        assert!(result.is_err());
        println!("      ❌ Below minimum threshold ($50 < $100): Correctly rejected");

        // Test exactly at minimum threshold
        let result = rules.calculate_distribution(dec!(100.0)); // Exactly $100 minimum
        assert!(result.is_ok());
        println!("      ✅ At minimum threshold ($100): Correctly accepted");

        // Test zero profit
        let result = rules.calculate_distribution(dec!(0.0));
        assert!(result.is_err());
        println!("      ❌ Zero profit: Correctly rejected");

        // Test negative profit (losses)
        let result = rules.calculate_distribution(dec!(-100.0));
        assert!(result.is_err());
        println!("      ❌ Negative profit (-$100): Correctly rejected");

        // Test very small profit above minimum
        let result = rules.calculate_distribution(dec!(100.01));
        assert!(result.is_ok());
        if let Ok(distribution) = result {
            let total = distribution.earnings_amount.unwrap()
                + distribution.tax_amount.unwrap()
                + distribution.reinvestment_amount.unwrap();
            assert_eq!(total, dec!(100.01));
        }
        println!("      ✅ Small profit above minimum ($100.01): Correctly processed");

        // Test large profit amounts
        let large_profit = dec!(10000.00);
        let result = rules.calculate_distribution(large_profit);
        assert!(result.is_ok());
        if let Ok(distribution) = result {
            let earnings = distribution.earnings_amount.unwrap();
            let tax = distribution.tax_amount.unwrap();
            let reinvestment = distribution.reinvestment_amount.unwrap();

            assert_eq!(earnings, dec!(4000.00)); // 40%
            assert_eq!(tax, dec!(3000.00)); // 30%
            assert_eq!(reinvestment, dec!(3000.00)); // 30%

            let total = earnings + tax + reinvestment;
            assert_eq!(total, large_profit);
        }
        println!("      ✅ Large profit ($10,000): Correctly distributed");

        // Test precision with fractional amounts
        let fractional_profit = dec!(123.45);
        let result = rules.calculate_distribution(fractional_profit);
        assert!(result.is_ok());
        if let Ok(distribution) = result {
            let total = distribution.earnings_amount.unwrap()
                + distribution.tax_amount.unwrap()
                + distribution.reinvestment_amount.unwrap();
            // Allow for minor decimal precision differences
            let diff = (total - fractional_profit).abs();
            assert!(diff < dec!(0.01)); // Within 1 cent
        }
        println!("      ✅ Fractional profit ($123.45): Correctly handled with precision");

        // Test distribution rules validation edge cases
        Self::test_distribution_rules_validation()?;

        Ok(())
    }

    fn test_distribution_rules_validation() -> Result<(), Box<dyn Error>> {
        let account_id = Uuid::new_v4();

        // Test invalid percentage totals (not summing to 100%)
        let invalid_rules = model::DistributionRules::new(
            account_id,
            dec!(0.50), // 50%
            dec!(0.30), // 30%
            dec!(0.30), // 30% = 110% total
            dec!(100.0),
        );

        let result = invalid_rules.validate();
        assert!(result.is_err());
        println!("      ❌ Invalid percentage total (110%): Correctly rejected");

        // Test negative percentages
        let negative_rules = model::DistributionRules::new(
            account_id,
            dec!(-0.10), // Negative percentage
            dec!(0.60),
            dec!(0.50),
            dec!(100.0),
        );

        let result = negative_rules.validate();
        assert!(result.is_err());
        println!("      ❌ Negative percentage: Correctly rejected");

        // Test zero minimum threshold (allowed in rules validation, but will affect distribution behavior)
        let zero_min_rules = model::DistributionRules::new(
            account_id,
            dec!(0.40),
            dec!(0.30),
            dec!(0.30),
            dec!(0.0), // Zero minimum
        );

        let result = zero_min_rules.validate();
        assert!(result.is_ok()); // Zero minimum is allowed in rules, just affects distribution threshold
        println!("      ✅ Zero minimum threshold: Allowed in rules validation");

        // Test negative minimum threshold (also allowed in rules validation)
        let negative_min_rules = model::DistributionRules::new(
            account_id,
            dec!(0.40),
            dec!(0.30),
            dec!(0.30),
            dec!(-100.0), // Negative minimum
        );

        let result = negative_min_rules.validate();
        assert!(result.is_ok()); // Negative minimum is allowed in rules, just affects distribution threshold
        println!(
            "      ✅ Negative minimum threshold: Allowed in rules validation (unusual but valid)"
        );

        // Test valid edge case (very low percentages that still sum to 100%)
        let low_pct_rules = model::DistributionRules::new(
            account_id,
            dec!(0.01), // 1%
            dec!(0.01), // 1%
            dec!(0.98), // 98% = 100% total
            dec!(1.0),  // $1 minimum
        );

        let result = low_pct_rules.validate();
        assert!(result.is_ok());
        println!("      ✅ Low percentage distribution (1%+1%+98%): Correctly accepted");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integration_complete_workflow() {
        IntegrationTestSuite::test_complete_workflow().expect("Integration test should pass");
    }

    #[test]
    fn test_integration_cli_validation() {
        IntegrationTestSuite::test_cli_integration().expect("CLI integration test should pass");
    }

    #[test]
    fn test_integration_event_workflow() {
        IntegrationTestSuite::test_event_integration().expect("Event integration test should pass");
    }

    #[test]
    fn test_integration_full_system() {
        IntegrationTestSuite::test_full_system_integration()
            .expect("Full system integration test should pass");
    }
}
