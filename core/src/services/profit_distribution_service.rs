use crate::services::fund_transfer_service::FundTransferService;
use model::{
    Account, Currency, DatabaseFactory, DistributionExecutionLeg, DistributionExecutionPlan,
    DistributionResult, DistributionRules, TransactionCategory,
};
use rust_decimal::Decimal;
use std::error::Error;
use uuid::Uuid;

/// Service for handling profit distribution across account hierarchy
pub struct ProfitDistributionService<'a> {
    database: &'a mut dyn DatabaseFactory,
}

impl<'a> std::fmt::Debug for ProfitDistributionService<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProfitDistributionService")
            .field("database", &"&mut dyn DatabaseFactory")
            .finish()
    }
}

impl<'a> ProfitDistributionService<'a> {
    /// Creates a new profit distribution service
    pub fn new(database: &'a mut dyn DatabaseFactory) -> Self {
        Self { database }
    }

    /// Calculates distribution amounts based on rules and profit amount
    pub fn calculate_distribution(
        &self,
        profit_amount: Decimal,
        rules: &DistributionRules,
    ) -> Result<DistributionResult, Box<dyn Error>> {
        // Delegate to DistributionRules which handles threshold validation
        rules
            .calculate_distribution(profit_amount)
            .map_err(Into::into)
    }

    /// Executes profit distribution across account hierarchy with atomic transactions
    #[allow(clippy::too_many_arguments)]
    pub fn execute_distribution(
        &mut self,
        source_account: &Account,
        earnings_account: &Account,
        tax_account: &Account,
        reinvestment_account: &Account,
        profit_amount: Decimal,
        rules: &DistributionRules,
        currency: &Currency,
        trade_id: Option<Uuid>,
    ) -> Result<DistributionResult, Box<dyn Error>> {
        // Calculate the distribution first
        let mut result = self.calculate_distribution(profit_amount, rules)?;

        // Update the source account ID to match the provided account
        result.source_account_id = source_account.id;

        let mut legs: Vec<DistributionExecutionLeg> = Vec::new();

        if let Some(amount) = result.earnings_amount {
            legs.push(DistributionExecutionLeg {
                to_account_id: earnings_account.id,
                amount,
                withdrawal_category: TransactionCategory::Withdrawal,
                deposit_category: trade_id
                    .map(TransactionCategory::PaymentEarnings)
                    .unwrap_or(TransactionCategory::Deposit),
                forced_withdrawal_tx_id: None,
                forced_deposit_tx_id: None,
            });
        }

        if let Some(amount) = result.tax_amount {
            legs.push(DistributionExecutionLeg {
                to_account_id: tax_account.id,
                amount,
                withdrawal_category: TransactionCategory::Withdrawal,
                deposit_category: trade_id
                    .map(TransactionCategory::PaymentTax)
                    .unwrap_or(TransactionCategory::Deposit),
                forced_withdrawal_tx_id: None,
                forced_deposit_tx_id: None,
            });
        }

        if let Some(amount) = result.reinvestment_amount {
            legs.push(DistributionExecutionLeg {
                to_account_id: reinvestment_account.id,
                amount,
                withdrawal_category: TransactionCategory::Withdrawal,
                deposit_category: TransactionCategory::Deposit,
                forced_withdrawal_tx_id: None,
                forced_deposit_tx_id: None,
            });
        }

        // Validate hierarchy constraints before any write.
        {
            let transfer_service = FundTransferService::new(self.database);
            for leg in &legs {
                let destination = if leg.to_account_id == earnings_account.id {
                    earnings_account
                } else if leg.to_account_id == tax_account.id {
                    tax_account
                } else if leg.to_account_id == reinvestment_account.id {
                    reinvestment_account
                } else {
                    return Err("Unknown distribution destination account".into());
                };
                transfer_service.validate_transfer(source_account, destination, leg.amount)?;
            }
        }

        let plan = DistributionExecutionPlan {
            source_account_id: source_account.id,
            currency: *currency,
            trade_id,
            original_amount: result.original_amount,
            distribution_date: result.distribution_date,
            legs,
            earnings_amount: result.earnings_amount,
            tax_amount: result.tax_amount,
            reinvestment_amount: result.reinvestment_amount,
        };

        let deposit_ids = self
            .database
            .distribution_write()
            .execute_distribution_plan_atomic(&plan)?;

        result.transactions_created = deposit_ids;
        Ok(result)
    }

    /// Transfers funds between accounts in hierarchy
    pub fn transfer_funds(
        &self,
        _from_account: &Account,
        _to_account: &Account,
        amount: Decimal,
        _reason: &str,
    ) -> Result<(), Box<dyn Error>> {
        // Validate transfer amount
        if amount <= Decimal::ZERO {
            return Err("Transfer amount cannot be negative or zero".into());
        }

        // For now, just validate the input and return success
        // Later this will create actual transactions
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::test_helpers::impl_database_factory_not_used;
    use chrono::Utc;
    use db_sqlite::SqliteDatabase;
    use model::{AccountType, DatabaseFactory};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    // Mock database factory for testing
    #[derive(Debug)]
    struct MockDatabaseFactory {
        #[allow(dead_code)]
        transactions_created: Vec<(Uuid, Uuid, Decimal)>, // (from, to, amount)
    }

    impl MockDatabaseFactory {
        fn new() -> Self {
            Self {
                transactions_created: Vec::new(),
            }
        }
    }

    impl_database_factory_not_used!(MockDatabaseFactory, "Mock not needed for this test");

    fn create_test_account(account_type: AccountType, parent_id: Option<Uuid>) -> Account {
        Account {
            id: Uuid::new_v4(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            deleted_at: None,
            name: "Test Account".to_string(),
            description: "Test account for distribution".to_string(),
            environment: model::Environment::Paper,
            taxes_percentage: dec!(25),
            earnings_percentage: dec!(30),
            account_type,
            parent_account_id: parent_id,
        }
    }

    fn create_test_distribution_rules(account_id: Uuid) -> DistributionRules {
        DistributionRules {
            id: Uuid::new_v4(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            account_id,
            earnings_percent: dec!(0.40),     // 40%
            tax_percent: dec!(0.30),          // 30%
            reinvestment_percent: dec!(0.30), // 30%
            minimum_threshold: dec!(100),
            configuration_password_hash: "test-password-hash".to_string(),
        }
    }

    fn create_real_hierarchy(
        database: &SqliteDatabase,
        prefix: &str,
    ) -> (Account, Account, Account, Account) {
        let source_account = database
            .account_write()
            .create_with_hierarchy(
                &format!("{prefix}-main"),
                &format!("{prefix}-main"),
                model::Environment::Paper,
                dec!(25),
                dec!(30),
                AccountType::Primary,
                None,
            )
            .expect("source account should be created");
        let earnings_account = database
            .account_write()
            .create_with_hierarchy(
                &format!("{prefix}-earnings"),
                &format!("{prefix}-earnings"),
                model::Environment::Paper,
                dec!(0),
                dec!(0),
                AccountType::Earnings,
                Some(source_account.id),
            )
            .expect("earnings account should be created");
        let tax_account = database
            .account_write()
            .create_with_hierarchy(
                &format!("{prefix}-tax"),
                &format!("{prefix}-tax"),
                model::Environment::Paper,
                dec!(0),
                dec!(0),
                AccountType::TaxReserve,
                Some(source_account.id),
            )
            .expect("tax account should be created");
        let reinvestment_account = database
            .account_write()
            .create_with_hierarchy(
                &format!("{prefix}-reinvest"),
                &format!("{prefix}-reinvest"),
                model::Environment::Paper,
                dec!(0),
                dec!(0),
                AccountType::Reinvestment,
                Some(source_account.id),
            )
            .expect("reinvestment account should be created");

        (
            source_account,
            earnings_account,
            tax_account,
            reinvestment_account,
        )
    }

    #[test]
    fn test_calculate_distribution_happy_path() {
        // Given: A profit distribution service
        let mut mock_db = MockDatabaseFactory::new();
        let service = ProfitDistributionService::new(&mut mock_db);

        // And: An account with distribution rules
        let account = create_test_account(AccountType::Primary, None);
        let rules = create_test_distribution_rules(account.id);

        // And: A profit amount above the minimum threshold
        let profit_amount = dec!(1000);

        // When: We calculate the distribution
        let result = service.calculate_distribution(profit_amount, &rules);

        // Then: The distribution should be calculated correctly
        let distribution = result.expect("Distribution calculation should succeed");
        assert_eq!(distribution.earnings_amount, Some(dec!(400))); // 40% of 1000
        assert_eq!(distribution.tax_amount, Some(dec!(300))); // 30% of 1000
        assert_eq!(distribution.reinvestment_amount, Some(dec!(300))); // 30% of 1000
        assert_eq!(distribution.original_amount, profit_amount);
    }

    #[test]
    fn test_calculate_distribution_below_threshold() {
        // Given: A profit distribution service
        let mut mock_db = MockDatabaseFactory::new();
        let service = ProfitDistributionService::new(&mut mock_db);

        // And: Distribution rules with minimum threshold of 100
        let account = create_test_account(AccountType::Primary, None);
        let rules = create_test_distribution_rules(account.id);

        // And: A profit amount below the threshold
        let profit_amount = dec!(50);

        // When: We calculate the distribution
        let result = service.calculate_distribution(profit_amount, &rules);

        // Then: No distribution should be calculated due to minimum threshold
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("below minimum threshold"));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_execute_distribution_with_actual_transfers() {
        // Given: A profit distribution service with real sqlite database
        let mut database = SqliteDatabase::new_in_memory();

        // And: Account hierarchy for distribution
        let (source_account, earnings_account, tax_account, reinvestment_account) =
            create_real_hierarchy(&database, "main");

        // And: Distribution rules and parameters
        let rules = create_test_distribution_rules(source_account.id);
        let profit_amount = dec!(1000);
        let currency = Currency::USD;

        // When: We execute the distribution with actual transfers
        let result = {
            let mut service = ProfitDistributionService::new(&mut database);
            service.execute_distribution(
                &source_account,
                &earnings_account,
                &tax_account,
                &reinvestment_account,
                profit_amount,
                &rules,
                &currency,
                None,
            )
        };

        // Then: The distribution should be executed successfully
        let distribution_result = result.expect("Distribution execution should succeed");
        assert_eq!(distribution_result.original_amount, profit_amount);
        assert_eq!(distribution_result.source_account_id, source_account.id);

        // Distribution amounts should match calculation
        assert_eq!(distribution_result.earnings_amount, Some(dec!(400)));
        assert_eq!(distribution_result.tax_amount, Some(dec!(300)));
        assert_eq!(distribution_result.reinvestment_amount, Some(dec!(300)));

        // Should have created 3 transactions (one for each allocation)
        assert_eq!(distribution_result.transactions_created.len(), 3);

        // And history row is persisted
        let history = database
            .distribution_read()
            .history_for_account(source_account.id)
            .expect("history should be readable");
        assert_eq!(history.len(), 1);
        let first_history = history.first().expect("history entry should exist");
        assert_eq!(first_history.original_amount, profit_amount);
    }

    #[test]
    fn test_transfer_funds_between_accounts() {
        // Given: A profit distribution service
        let mut mock_db = MockDatabaseFactory::new();
        let service = ProfitDistributionService::new(&mut mock_db);

        // And: Two accounts in the same hierarchy
        let parent_account = create_test_account(AccountType::Primary, None);
        let child_account = create_test_account(AccountType::Earnings, Some(parent_account.id));

        // And: A transfer amount and reason
        let transfer_amount = dec!(500);
        let reason = "Test transfer for earnings distribution";

        // When: We transfer funds between accounts
        let result =
            service.transfer_funds(&parent_account, &child_account, transfer_amount, reason);

        // Then: The transfer should succeed
        assert!(result.is_ok(), "Fund transfer should succeed");
    }

    #[test]
    fn test_transfer_funds_with_negative_amount_fails() {
        // Given: A profit distribution service
        let mut mock_db = MockDatabaseFactory::new();
        let service = ProfitDistributionService::new(&mut mock_db);

        // And: Two valid accounts
        let from_account = create_test_account(AccountType::Primary, None);
        let to_account = create_test_account(AccountType::Earnings, Some(from_account.id));

        // And: A negative transfer amount
        let negative_amount = dec!(-100);

        // When: We attempt to transfer negative amount
        let result = service.transfer_funds(
            &from_account,
            &to_account,
            negative_amount,
            "Invalid transfer",
        );

        // Then: The transfer should fail
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("negative") || error_msg.contains("invalid"));
    }

    #[test]
    fn test_execute_distribution_invalid_hierarchy_fails() {
        // Given: A profit distribution service with database
        let mut mock_db = MockDatabaseFactory::new();
        let mut service = ProfitDistributionService::new(&mut mock_db);

        // And: Accounts with invalid hierarchy (unrelated accounts)
        let source_account = create_test_account(AccountType::Primary, None);
        let unrelated_account = create_test_account(AccountType::Primary, None); // Different primary account
        let earnings_account = create_test_account(AccountType::Earnings, Some(source_account.id));
        let tax_account = create_test_account(AccountType::TaxReserve, Some(source_account.id));

        // And: Distribution rules and parameters
        let rules = create_test_distribution_rules(source_account.id);
        let profit_amount = dec!(1000);
        let currency = Currency::USD;

        // When: We execute the distribution with invalid hierarchy
        let result = service.execute_distribution(
            &source_account,
            &unrelated_account, // This should fail - no hierarchy relationship
            &tax_account,
            &earnings_account,
            profit_amount,
            &rules,
            &currency,
            None,
        );

        // Then: The distribution should fail due to hierarchy validation
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("hierarchy") || error_msg.contains("relationship"));
    }

    #[test]
    fn test_validate_distribution_accounts_success() {
        // Given: A profit distribution service with database
        let mut mock_db = MockDatabaseFactory::new();
        let _service = ProfitDistributionService::new(&mut mock_db);

        // And: A valid account hierarchy
        let source_account = create_test_account(AccountType::Primary, None);
        let earnings_account = create_test_account(AccountType::Earnings, Some(source_account.id));
        let tax_account = create_test_account(AccountType::TaxReserve, Some(source_account.id));
        let reinvestment_account =
            create_test_account(AccountType::Reinvestment, Some(source_account.id));

        // When: We validate using the fund transfer service directly
        let transfer_service = FundTransferService::new(&mut mock_db);
        let validation_amount = dec!(1.0);

        // Then: Each validation should succeed
        assert!(transfer_service
            .validate_transfer(&source_account, &earnings_account, validation_amount)
            .is_ok());
        assert!(transfer_service
            .validate_transfer(&source_account, &tax_account, validation_amount)
            .is_ok());
        assert!(transfer_service
            .validate_transfer(&source_account, &reinvestment_account, validation_amount)
            .is_ok());
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_execute_distribution_with_zero_amounts() {
        // Given: A profit distribution service with real sqlite database
        let mut database = SqliteDatabase::new_in_memory();

        // And: Account hierarchy for distribution
        let (source_account, earnings_account, tax_account, reinvestment_account) =
            create_real_hierarchy(&database, "zero");

        // And: Distribution rules with zero percentages for some allocations
        let mut rules = create_test_distribution_rules(source_account.id);
        rules.earnings_percent = dec!(1.00); // 100% to earnings only
        rules.tax_percent = dec!(0.00); // 0% to tax
        rules.reinvestment_percent = dec!(0.00); // 0% to reinvestment

        let profit_amount = dec!(1000);
        let currency = Currency::USD;

        // When: We execute the distribution with zero amounts
        let result = {
            let mut service = ProfitDistributionService::new(&mut database);
            service.execute_distribution(
                &source_account,
                &earnings_account,
                &tax_account,
                &reinvestment_account,
                profit_amount,
                &rules,
                &currency,
                None,
            )
        };

        // Then: The distribution should succeed with only earnings transfer
        let distribution_result = result.expect("Distribution execution should succeed");
        assert_eq!(distribution_result.earnings_amount, Some(dec!(1000))); // 100% to earnings
        assert_eq!(distribution_result.tax_amount, None); // 0% to tax
        assert_eq!(distribution_result.reinvestment_amount, None); // 0% to reinvestment

        // Should have created only 1 transaction (earnings only)
        assert_eq!(distribution_result.transactions_created.len(), 1);

        // History is still persisted even with zero-value allocation legs
        let history = database
            .distribution_read()
            .history_for_account(source_account.id)
            .expect("history should be readable");
        assert_eq!(history.len(), 1);
        let first_history = history.first().expect("history entry should exist");
        assert_eq!(first_history.earnings_amount, Some(dec!(1000)));
        assert_eq!(first_history.tax_amount, None);
        assert_eq!(first_history.reinvestment_amount, None);
    }
}
