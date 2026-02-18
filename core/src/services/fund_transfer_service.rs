use model::{Account, Currency, DatabaseFactory, TransactionCategory};
use rust_decimal::Decimal;
use std::error::Error;
use uuid::Uuid;

/// Service for handling fund transfers between accounts
pub struct FundTransferService<'a> {
    database: &'a mut dyn DatabaseFactory,
}

impl<'a> std::fmt::Debug for FundTransferService<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FundTransferService")
            .field("database", &"&mut dyn DatabaseFactory")
            .finish()
    }
}

impl<'a> FundTransferService<'a> {
    /// Creates a new fund transfer service
    pub fn new(database: &'a mut dyn DatabaseFactory) -> Self {
        Self { database }
    }

    /// Transfers funds between accounts with transaction records
    pub fn transfer_between_accounts(
        &mut self,
        from_account: &Account,
        to_account: &Account,
        amount: Decimal,
        currency: &Currency,
        _reason: &str,
    ) -> Result<(Uuid, Uuid), Box<dyn Error>> {
        // Validate the transfer first
        self.validate_transfer(from_account, to_account, amount)?;
        let (withdrawal_tx, deposit_tx) = self.database.transaction_write().create_transfer_pair(
            from_account,
            to_account,
            amount,
            currency,
            TransactionCategory::Withdrawal,
            TransactionCategory::Deposit,
        )?;

        Ok((withdrawal_tx.id, deposit_tx.id))
    }

    /// Validates if a transfer is allowed between two accounts
    pub fn validate_transfer(
        &self,
        from_account: &Account,
        to_account: &Account,
        amount: Decimal,
    ) -> Result<(), Box<dyn Error>> {
        // Validate amount is positive
        if amount <= Decimal::ZERO {
            return Err("Transfer amount must be positive".into());
        }

        if from_account.id == to_account.id {
            return Err("Cannot transfer funds to the same account".into());
        }

        // Validate accounts have hierarchy relationship
        let accounts_related = from_account.id == to_account.parent_account_id.unwrap_or_default()
            || to_account.id == from_account.parent_account_id.unwrap_or_default()
            || (from_account.parent_account_id.is_some()
                && to_account.parent_account_id.is_some()
                && from_account.parent_account_id == to_account.parent_account_id);

        if !accounts_related {
            return Err("Accounts must have a hierarchy relationship".into());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use db_sqlite::SqliteDatabase;
    use model::database::WriteAccountBalanceDB;
    use model::{AccountType, DatabaseFactory, Environment};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    // Mock database factory for testing
    #[derive(Debug)]
    struct MockDatabaseFactory {
        #[allow(dead_code)]
        transactions_created: Vec<(Uuid, TransactionCategory, Decimal)>,
    }

    impl MockDatabaseFactory {
        fn new() -> Self {
            Self {
                transactions_created: Vec::new(),
            }
        }
    }

    // Implement required traits for mock - simplified for testing
    impl DatabaseFactory for MockDatabaseFactory {
        fn account_read(&self) -> Box<dyn model::AccountRead> {
            todo!("Mock not needed for this test")
        }

        fn account_write(&self) -> Box<dyn model::AccountWrite> {
            todo!("Mock not needed for this test")
        }

        fn account_balance_read(&self) -> Box<dyn model::AccountBalanceRead> {
            todo!("Mock not needed for this test")
        }

        fn account_balance_write(&self) -> Box<dyn model::AccountBalanceWrite> {
            todo!("Mock not needed for this test")
        }

        fn order_read(&self) -> Box<dyn model::OrderRead> {
            todo!("Mock not needed for this test")
        }

        fn order_write(&self) -> Box<dyn model::OrderWrite> {
            todo!("Mock not needed for this test")
        }

        fn transaction_read(&self) -> Box<dyn model::ReadTransactionDB> {
            todo!("Mock not needed for this test")
        }

        fn transaction_write(&self) -> Box<dyn model::WriteTransactionDB> {
            todo!("Mock not needed for this test")
        }

        fn trade_read(&self) -> Box<dyn model::ReadTradeDB> {
            todo!("Mock not needed for this test")
        }

        fn trade_write(&self) -> Box<dyn model::WriteTradeDB> {
            todo!("Mock not needed for this test")
        }

        fn trade_balance_write(&self) -> Box<dyn WriteAccountBalanceDB> {
            todo!("Mock not needed for this test")
        }

        fn rule_read(&self) -> Box<dyn model::ReadRuleDB> {
            todo!("Mock not needed for this test")
        }

        fn rule_write(&self) -> Box<dyn model::WriteRuleDB> {
            todo!("Mock not needed for this test")
        }

        fn trading_vehicle_read(&self) -> Box<dyn model::ReadTradingVehicleDB> {
            todo!("Mock not needed for this test")
        }

        fn trading_vehicle_write(&self) -> Box<dyn model::WriteTradingVehicleDB> {
            todo!("Mock not needed for this test")
        }

        fn log_read(&self) -> Box<dyn model::ReadBrokerLogsDB> {
            todo!("Mock not needed for this test")
        }

        fn log_write(&self) -> Box<dyn model::WriteBrokerLogsDB> {
            todo!("Mock not needed for this test")
        }

        fn broker_event_read(&self) -> Box<dyn model::ReadBrokerEventsDB> {
            todo!("Mock not needed for this test")
        }

        fn broker_event_write(&self) -> Box<dyn model::WriteBrokerEventsDB> {
            todo!("Mock not needed for this test")
        }

        fn distribution_read(&self) -> Box<dyn model::DistributionRead> {
            #[allow(unused_imports)]
            use model::database::DistributionRead;
            todo!("Mock not needed for this test")
        }

        fn distribution_write(&self) -> Box<dyn model::DistributionWrite> {
            #[allow(unused_imports)]
            use model::database::DistributionWrite;
            todo!("Mock not needed for this test")
        }

        fn execution_read(&self) -> Box<dyn model::ReadExecutionDB> {
            todo!("Mock not needed for this test")
        }

        fn execution_write(&self) -> Box<dyn model::WriteExecutionDB> {
            todo!("Mock not needed for this test")
        }

        fn trade_grade_read(&self) -> Box<dyn model::ReadTradeGradeDB> {
            todo!("Mock not needed for this test")
        }

        fn trade_grade_write(&self) -> Box<dyn model::WriteTradeGradeDB> {
            todo!("Mock not needed for this test")
        }

        fn level_read(&self) -> Box<dyn model::ReadLevelDB> {
            todo!("Mock not needed for this test")
        }

        fn level_write(&self) -> Box<dyn model::WriteLevelDB> {
            todo!("Mock not needed for this test")
        }

        fn begin_savepoint(&mut self, _name: &str) -> Result<(), Box<dyn Error>> {
            Ok(())
        }

        fn release_savepoint(&mut self, _name: &str) -> Result<(), Box<dyn Error>> {
            Ok(())
        }

        fn rollback_to_savepoint(&mut self, _name: &str) -> Result<(), Box<dyn Error>> {
            Ok(())
        }
    }

    fn create_test_account(account_type: AccountType, parent_id: Option<Uuid>) -> Account {
        Account {
            id: Uuid::new_v4(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            deleted_at: None,
            name: "Test Account".to_string(),
            description: "Test account for transfers".to_string(),
            environment: Environment::Paper,
            taxes_percentage: dec!(25),
            earnings_percentage: dec!(30),
            account_type,
            parent_account_id: parent_id,
        }
    }

    #[test]
    fn test_transfer_between_accounts_creates_transactions() {
        // Given: A fund transfer service with real sqlite database
        let mut database = SqliteDatabase::new_in_memory();

        // And: Two accounts for transfer in a real hierarchy
        let from_account = database
            .account_write()
            .create_with_hierarchy(
                "main",
                "main",
                Environment::Paper,
                dec!(25),
                dec!(30),
                AccountType::Primary,
                None,
            )
            .expect("source account should be created");
        let to_account = database
            .account_write()
            .create_with_hierarchy(
                "earnings",
                "earnings",
                Environment::Paper,
                dec!(0),
                dec!(0),
                AccountType::Earnings,
                Some(from_account.id),
            )
            .expect("child account should be created");

        // And: Transfer parameters
        let amount = dec!(500);
        let currency = Currency::USD;
        let reason = "Profit distribution transfer";

        // When: We transfer funds between accounts
        let result = {
            let mut service = FundTransferService::new(&mut database);
            service.transfer_between_accounts(&from_account, &to_account, amount, &currency, reason)
        };

        // Then: The transfer should succeed and return transaction IDs
        assert!(result.is_ok(), "Fund transfer should succeed");
        let (withdrawal_tx_id, deposit_tx_id) = result.unwrap();
        assert_ne!(
            withdrawal_tx_id, deposit_tx_id,
            "Transaction IDs should be different"
        );

        // And: Both transactions are persisted
        let source_transactions = database
            .transaction_read()
            .all_transactions(from_account.id, &currency)
            .expect("source transactions should be readable");
        let child_transactions = database
            .transaction_read()
            .all_transactions(to_account.id, &currency)
            .expect("child transactions should be readable");
        assert_eq!(source_transactions.len(), 1);
        assert_eq!(child_transactions.len(), 1);
    }

    #[test]
    fn test_transfer_between_accounts_rolls_back_if_deposit_write_fails() {
        // This is a regression test for atomicity: if the deposit leg fails, the withdrawal
        // must not remain committed.

        let mut database = SqliteDatabase::new_in_memory();

        let from_account = database
            .account_write()
            .create_with_hierarchy(
                "main",
                "main",
                Environment::Paper,
                dec!(25),
                dec!(30),
                AccountType::Primary,
                None,
            )
            .expect("source account should be created");

        // Create a destination Account value that is related (passes validation),
        // but does not exist in the DB, to force the deposit insert to fail via FK constraint.
        let to_account = Account {
            id: Uuid::new_v4(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            deleted_at: None,
            name: "missing-child".to_string(),
            description: "missing child".to_string(),
            environment: Environment::Paper,
            taxes_percentage: dec!(0),
            earnings_percentage: dec!(0),
            account_type: AccountType::Earnings,
            parent_account_id: Some(from_account.id),
        };

        let amount = dec!(500);
        let currency = Currency::USD;

        let result = {
            let mut service = FundTransferService::new(&mut database);
            service.transfer_between_accounts(
                &from_account,
                &to_account,
                amount,
                &currency,
                "atomicity regression test",
            )
        };

        assert!(result.is_err());

        let source_transactions = database
            .transaction_read()
            .all_transactions(from_account.id, &currency)
            .expect("source transactions should be readable");
        assert_eq!(
            source_transactions.len(),
            0,
            "withdrawal leg must be rolled back on deposit failure"
        );
    }

    #[test]
    fn test_validate_transfer_valid_hierarchy() {
        // Given: A fund transfer service
        let mut mock_db = MockDatabaseFactory::new();
        let service = FundTransferService::new(&mut mock_db);

        // And: Valid parent-child account relationship
        let parent_account = create_test_account(AccountType::Primary, None);
        let child_account = create_test_account(AccountType::Earnings, Some(parent_account.id));

        // And: Valid transfer amount
        let amount = dec!(100);

        // When: We validate the transfer
        let result = service.validate_transfer(&parent_account, &child_account, amount);

        // Then: The validation should succeed
        assert!(
            result.is_ok(),
            "Transfer validation should succeed for valid hierarchy"
        );
    }

    #[test]
    fn test_validate_transfer_invalid_amount() {
        // Given: A fund transfer service
        let mut mock_db = MockDatabaseFactory::new();
        let service = FundTransferService::new(&mut mock_db);

        // And: Valid accounts
        let from_account = create_test_account(AccountType::Primary, None);
        let to_account = create_test_account(AccountType::Earnings, Some(from_account.id));

        // And: Invalid (negative) transfer amount
        let amount = dec!(-50);

        // When: We validate the transfer
        let result = service.validate_transfer(&from_account, &to_account, amount);

        // Then: The validation should fail
        assert!(
            result.is_err(),
            "Transfer validation should fail for negative amount"
        );
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("positive") || error_msg.contains("negative"));
    }

    #[test]
    fn test_validate_transfer_no_hierarchy_relationship() {
        // Given: A fund transfer service
        let mut mock_db = MockDatabaseFactory::new();
        let service = FundTransferService::new(&mut mock_db);

        // And: Two unrelated accounts (no parent-child relationship)
        let account1 = create_test_account(AccountType::Primary, None);
        let account2 = create_test_account(AccountType::Primary, None); // Different primary account

        // And: Valid transfer amount
        let amount = dec!(100);

        // When: We validate the transfer
        let result = service.validate_transfer(&account1, &account2, amount);

        // Then: The validation should fail
        assert!(
            result.is_err(),
            "Transfer validation should fail for unrelated accounts"
        );
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("hierarchy") || error_msg.contains("relationship"));
    }

    #[test]
    fn test_validate_transfer_same_account_rejected() {
        let mut mock_db = MockDatabaseFactory::new();
        let service = FundTransferService::new(&mut mock_db);
        let account = create_test_account(AccountType::Primary, None);

        let result = service.validate_transfer(&account, &account, dec!(100));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("same account"));
    }
}
