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

struct DistributionTargetRefs<'a> {
    earnings: &'a Account,
    tax: &'a Account,
    reinvestment: &'a Account,
    insurance: Option<&'a Account>,
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
        insurance_account: Option<&Account>,
        profit_amount: Decimal,
        rules: &DistributionRules,
        currency: &Currency,
        trade_id: Option<Uuid>,
    ) -> Result<DistributionResult, Box<dyn Error>> {
        // Calculate the distribution first
        let mut result = self.calculate_distribution(profit_amount, rules)?;

        // Update the source account ID to match the provided account
        result.source_account_id = source_account.id;

        let target_accounts = DistributionTargetRefs {
            earnings: earnings_account,
            tax: tax_account,
            reinvestment: reinvestment_account,
            insurance: insurance_account,
        };
        let legs = build_distribution_legs(&result, &target_accounts, trade_id)?;
        validate_distribution_legs(self.database, source_account, &target_accounts, &legs)?;

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
            insurance_amount: result.insurance_amount,
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

fn build_distribution_legs(
    result: &DistributionResult,
    targets: &DistributionTargetRefs<'_>,
    trade_id: Option<Uuid>,
) -> Result<Vec<DistributionExecutionLeg>, Box<dyn Error>> {
    let mut legs: Vec<DistributionExecutionLeg> = Vec::new();

    if let Some(amount) = result.earnings_amount {
        push_distribution_leg(
            &mut legs,
            targets.earnings.id,
            amount,
            trade_id
                .map(TransactionCategory::PaymentEarnings)
                .unwrap_or(TransactionCategory::Deposit),
        );
    }

    if let Some(amount) = result.tax_amount {
        push_distribution_leg(
            &mut legs,
            targets.tax.id,
            amount,
            trade_id
                .map(TransactionCategory::PaymentTax)
                .unwrap_or(TransactionCategory::Deposit),
        );
    }

    if let Some(amount) = result.reinvestment_amount {
        push_distribution_leg(
            &mut legs,
            targets.reinvestment.id,
            amount,
            TransactionCategory::Deposit,
        );
    }

    if let Some(amount) = result.insurance_amount {
        let account = targets
            .insurance
            .ok_or("Missing insurance subaccount for distribution")?;
        push_distribution_leg(&mut legs, account.id, amount, TransactionCategory::Deposit);
    }

    Ok(legs)
}

fn push_distribution_leg(
    legs: &mut Vec<DistributionExecutionLeg>,
    to_account_id: Uuid,
    amount: Decimal,
    deposit_category: TransactionCategory,
) {
    legs.push(DistributionExecutionLeg {
        to_account_id,
        amount,
        withdrawal_category: TransactionCategory::Withdrawal,
        deposit_category,
        forced_withdrawal_tx_id: None,
        forced_deposit_tx_id: None,
    });
}

fn validate_distribution_legs(
    database: &mut dyn DatabaseFactory,
    source_account: &Account,
    targets: &DistributionTargetRefs<'_>,
    legs: &[DistributionExecutionLeg],
) -> Result<(), Box<dyn Error>> {
    let transfer_service = FundTransferService::new(database);
    for leg in legs {
        let destination = distribution_destination(leg, targets)?;
        transfer_service.validate_transfer(source_account, destination, leg.amount)?;
    }
    Ok(())
}

fn distribution_destination<'a>(
    leg: &DistributionExecutionLeg,
    targets: &'a DistributionTargetRefs<'a>,
) -> Result<&'a Account, Box<dyn Error>> {
    if leg.to_account_id == targets.earnings.id {
        Ok(targets.earnings)
    } else if leg.to_account_id == targets.tax.id {
        Ok(targets.tax)
    } else if leg.to_account_id == targets.reinvestment.id {
        Ok(targets.reinvestment)
    } else if let Some(account) = targets.insurance {
        if leg.to_account_id == account.id {
            Ok(account)
        } else {
            Err("Unknown distribution destination account".into())
        }
    } else {
        Err("Unknown distribution destination account".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use db_sqlite::SqliteDatabase;
    use model::AccountType;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

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
            broker_kind: model::BrokerKind::Alpaca,
            broker_account_id: None,
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
            insurance_percent: Decimal::ZERO, // 0%
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

    fn create_real_child_account(
        database: &SqliteDatabase,
        source_account: &Account,
        name: &str,
        account_type: AccountType,
    ) -> Account {
        database
            .account_write()
            .create_with_hierarchy(
                name,
                name,
                model::Environment::Paper,
                dec!(0),
                dec!(0),
                account_type,
                Some(source_account.id),
            )
            .expect("child account should be created")
    }

    #[test]
    fn test_debug_redacts_database_trait_object() {
        let mut database = SqliteDatabase::new_in_memory();
        let service = ProfitDistributionService::new(&mut database);

        assert_eq!(
            format!("{service:?}"),
            "ProfitDistributionService { database: \"&mut dyn DatabaseFactory\" }"
        );
    }

    fn create_persisted_trade(database: &mut SqliteDatabase, account: &Account) -> model::Trade {
        let vehicle = database
            .trading_vehicle_write()
            .create_trading_vehicle(
                "DISTLINK",
                Some("DISTLINK"),
                &model::TradingVehicleCategory::Stock,
                "alpaca",
            )
            .expect("trading vehicle should be created");
        let stop = database
            .order_write()
            .create(
                &vehicle,
                dec!(10),
                dec!(90),
                &Currency::USD,
                &model::OrderAction::Sell,
                &model::OrderCategory::Stop,
            )
            .expect("stop order should be created");
        let entry = database
            .order_write()
            .create(
                &vehicle,
                dec!(10),
                dec!(100),
                &Currency::USD,
                &model::OrderAction::Buy,
                &model::OrderCategory::Limit,
            )
            .expect("entry order should be created");
        let target = database
            .order_write()
            .create(
                &vehicle,
                dec!(10),
                dec!(120),
                &Currency::USD,
                &model::OrderAction::Sell,
                &model::OrderCategory::Limit,
            )
            .expect("target order should be created");
        let draft = model::DraftTrade {
            account: account.clone(),
            trading_vehicle: vehicle,
            quantity: 10.into(),
            currency: Currency::USD,
            category: model::TradeCategory::Long,
            thesis: None,
            sector: None,
            asset_class: None,
            context: None,
        };

        database
            .trade_write()
            .create_trade(draft, &stop, &entry, &target)
            .expect("trade should be created")
    }

    fn account_transactions(
        database: &SqliteDatabase,
        account_id: Uuid,
        currency: &Currency,
        _label: &str,
    ) -> Vec<model::Transaction> {
        database
            .transaction_read()
            .all_transactions(account_id, currency)
            .expect("transactions should be readable")
    }

    fn assert_source_withdrawals(transactions: &[model::Transaction], expected_total: Decimal) {
        assert_eq!(transactions.len(), 3);
        assert!(transactions
            .iter()
            .all(|tx| tx.category == TransactionCategory::Withdrawal));
        let source_total = transactions
            .iter()
            .try_fold(Decimal::ZERO, |total, tx| total.checked_add(tx.amount))
            .expect("transaction total should not overflow");
        assert_eq!(source_total, expected_total);
    }

    fn assert_destination_transaction(
        transactions: &[model::Transaction],
        expected_category: TransactionCategory,
        expected_amount: Decimal,
        _label: &str,
    ) -> Uuid {
        assert_eq!(transactions.len(), 1);
        let transaction = transactions
            .first()
            .expect("destination transaction should exist");
        assert_eq!(transaction.category, expected_category);
        assert_eq!(transaction.amount, expected_amount);
        transaction.id
    }

    #[test]
    fn test_calculate_distribution_happy_path() {
        // Given: A profit distribution service
        let mut database = SqliteDatabase::new_in_memory();
        let service = ProfitDistributionService::new(&mut database);

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
        let mut database = SqliteDatabase::new_in_memory();
        let service = ProfitDistributionService::new(&mut database);

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
                None,
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
        let mut database = SqliteDatabase::new_in_memory();
        let service = ProfitDistributionService::new(&mut database);

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
        let mut database = SqliteDatabase::new_in_memory();
        let service = ProfitDistributionService::new(&mut database);

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
        let mut database = SqliteDatabase::new_in_memory();
        let mut service = ProfitDistributionService::new(&mut database);

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
            None,
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
        // Given: A valid account hierarchy
        let mut database = SqliteDatabase::new_in_memory();
        let source_account = create_test_account(AccountType::Primary, None);
        let earnings_account = create_test_account(AccountType::Earnings, Some(source_account.id));
        let tax_account = create_test_account(AccountType::TaxReserve, Some(source_account.id));
        let reinvestment_account =
            create_test_account(AccountType::Reinvestment, Some(source_account.id));

        // When: We validate using the fund transfer service directly
        let transfer_service = FundTransferService::new(&mut database);
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
                None,
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
        assert_eq!(first_history.insurance_amount, None);
    }

    #[test]
    fn test_execute_distribution_allocates_insurance_amount() {
        let mut database = SqliteDatabase::new_in_memory();
        let (source_account, earnings_account, tax_account, reinvestment_account) =
            create_real_hierarchy(&database, "insurance");
        let insurance_account = create_real_child_account(
            &database,
            &source_account,
            "insurance-child",
            AccountType::Insurance,
        );

        let mut rules = create_test_distribution_rules(source_account.id);
        rules.earnings_percent = dec!(0.35);
        rules.tax_percent = dec!(0.25);
        rules.reinvestment_percent = dec!(0.30);
        rules.insurance_percent = dec!(0.10);

        let result = {
            let mut service = ProfitDistributionService::new(&mut database);
            service.execute_distribution(
                &source_account,
                &earnings_account,
                &tax_account,
                &reinvestment_account,
                Some(&insurance_account),
                dec!(1000),
                &rules,
                &Currency::USD,
                None,
            )
        }
        .expect("insurance distribution should execute");

        assert_eq!(result.insurance_amount, Some(dec!(100)));
        assert_eq!(result.transactions_created.len(), 4);

        let insurance_transactions =
            account_transactions(&database, insurance_account.id, &Currency::USD, "insurance");
        assert_destination_transaction(
            &insurance_transactions,
            TransactionCategory::Deposit,
            dec!(100),
            "insurance",
        );

        let history = database
            .distribution_read()
            .history_for_account(source_account.id)
            .expect("history should be readable");
        let history = history.first().expect("history row should exist");
        assert_eq!(history.insurance_amount, Some(dec!(100)));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_execute_distribution_with_trade_id_uses_auditable_payment_categories() {
        let mut database = SqliteDatabase::new_in_memory();
        let (source_account, earnings_account, tax_account, reinvestment_account) =
            create_real_hierarchy(&database, "trade-linked");
        let rules = create_test_distribution_rules(source_account.id);
        let profit_amount = dec!(1000);
        let currency = Currency::USD;
        let trade_id = create_persisted_trade(&mut database, &source_account).id;

        let distribution_result = {
            let mut service = ProfitDistributionService::new(&mut database);
            service
                .execute_distribution(
                    &source_account,
                    &earnings_account,
                    &tax_account,
                    &reinvestment_account,
                    None,
                    profit_amount,
                    &rules,
                    &currency,
                    Some(trade_id),
                )
                .expect("trade-linked distribution should execute")
        };

        assert_eq!(distribution_result.transactions_created.len(), 3);

        let source_transactions =
            account_transactions(&database, source_account.id, &currency, "source");
        assert_source_withdrawals(&source_transactions, dec!(-1000));

        let earnings_transactions =
            account_transactions(&database, earnings_account.id, &currency, "earnings");
        let tax_transactions = account_transactions(&database, tax_account.id, &currency, "tax");
        let reinvestment_transactions = account_transactions(
            &database,
            reinvestment_account.id,
            &currency,
            "reinvestment",
        );

        let destination_transaction_ids = [
            assert_destination_transaction(
                &earnings_transactions,
                TransactionCategory::PaymentEarnings(trade_id),
                dec!(400),
                "earnings",
            ),
            assert_destination_transaction(
                &tax_transactions,
                TransactionCategory::PaymentTax(trade_id),
                dec!(300),
                "tax",
            ),
            assert_destination_transaction(
                &reinvestment_transactions,
                TransactionCategory::Deposit,
                dec!(300),
                "reinvestment",
            ),
        ];
        for transaction_id in destination_transaction_ids {
            assert!(distribution_result
                .transactions_created
                .contains(&transaction_id));
        }

        let history = database
            .distribution_read()
            .history_for_account(source_account.id)
            .expect("history should be readable");
        assert_eq!(history.len(), 1);
        let history = history.first().expect("history row should exist");
        assert_eq!(history.trade_id, Some(trade_id));
        assert_eq!(history.original_amount, profit_amount);
        assert_eq!(history.earnings_amount, Some(dec!(400)));
        assert_eq!(history.tax_amount, Some(dec!(300)));
        assert_eq!(history.reinvestment_amount, Some(dec!(300)));
        assert_eq!(history.insurance_amount, None);
    }
}
