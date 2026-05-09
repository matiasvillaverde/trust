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
    use model::{AccountType, Environment};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

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
            broker_kind: model::BrokerKind::Alpaca,
            broker_account_id: None,
        }
    }

    fn sqlite_service(database: &mut SqliteDatabase) -> FundTransferService<'_> {
        FundTransferService::new(database)
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

        let source_transaction = source_transactions
            .first()
            .expect("source transaction should exist");
        assert_eq!(source_transaction.id, withdrawal_tx_id);
        assert_eq!(source_transaction.category, TransactionCategory::Withdrawal);
        assert_eq!(source_transaction.amount, dec!(-500));

        let child_transaction = child_transactions
            .first()
            .expect("child transaction should exist");
        assert_eq!(child_transaction.id, deposit_tx_id);
        assert_eq!(child_transaction.category, TransactionCategory::Deposit);
        assert_eq!(child_transaction.amount, dec!(500));
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
            broker_kind: model::BrokerKind::Alpaca,
            broker_account_id: None,
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
        let mut database = SqliteDatabase::new_in_memory();
        let service = sqlite_service(&mut database);

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
    fn test_validate_transfer_allows_child_to_parent_and_sibling_transfers() {
        let mut database = SqliteDatabase::new_in_memory();
        let service = sqlite_service(&mut database);

        let parent_account = create_test_account(AccountType::Primary, None);
        let earnings_account = create_test_account(AccountType::Earnings, Some(parent_account.id));
        let tax_account = create_test_account(AccountType::TaxReserve, Some(parent_account.id));

        assert!(service
            .validate_transfer(&earnings_account, &parent_account, dec!(25))
            .is_ok());
        assert!(service
            .validate_transfer(&earnings_account, &tax_account, dec!(25))
            .is_ok());
    }

    #[test]
    fn test_validate_transfer_invalid_amount() {
        // Given: A fund transfer service
        let mut database = SqliteDatabase::new_in_memory();
        let service = sqlite_service(&mut database);

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
        let mut database = SqliteDatabase::new_in_memory();
        let service = sqlite_service(&mut database);

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
        let mut database = SqliteDatabase::new_in_memory();
        let service = sqlite_service(&mut database);
        let account = create_test_account(AccountType::Primary, None);

        let result = service.validate_transfer(&account, &account, dec!(100));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("same account"));
    }

    #[test]
    fn debug_representation_does_not_expose_database_internals() {
        let mut database = SqliteDatabase::new_in_memory();
        let service = sqlite_service(&mut database);

        assert_eq!(
            format!("{service:?}"),
            "FundTransferService { database: \"&mut dyn DatabaseFactory\" }"
        );
    }
}
