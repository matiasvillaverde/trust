use crate::services::ProfitDistributionService;
use model::{Account, AccountType, Currency, DatabaseFactory, DistributionRulesNotFound, Trade};
use rust_decimal::Decimal;
use std::error::Error;

/// Service for handling event-driven automatic profit distribution
/// Listens to trade closure events and triggers distribution when profitable
pub struct EventDistributionService<'a> {
    database: &'a mut dyn DatabaseFactory,
}

impl<'a> std::fmt::Debug for EventDistributionService<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventDistributionService")
            .field("database", &"&mut dyn DatabaseFactory")
            .finish()
    }
}

impl<'a> EventDistributionService<'a> {
    /// Creates a new event distribution service
    pub fn new(database: &'a mut dyn DatabaseFactory) -> Self {
        Self { database }
    }

    /// Handles trade closure event and triggers automatic distribution if profitable
    pub fn handle_trade_closed_event(
        &mut self,
        trade: &Trade,
        currency: &Currency,
    ) -> Result<Option<model::DistributionResult>, Box<dyn Error>> {
        // 1. Check if trade was profitable
        let profit = self.calculate_trade_profit(trade)?;
        if profit <= Decimal::ZERO {
            return Ok(None); // No distribution for losses
        }

        let source_account = self.database.account_read().id(trade.account_id)?;
        let rules = match distribution_rules_or_none(
            self.database
                .distribution_read()
                .for_account(trade.account_id),
        )? {
            Some(rules) => rules,
            None => return Ok(None),
        };

        // Rules exist, but threshold can still opt-out.
        if profit < rules.minimum_threshold {
            return Ok(None);
        }

        let (earnings_account, tax_account, reinvestment_account) =
            self.find_distribution_accounts(source_account.id)?;
        let mut distribution_service = ProfitDistributionService::new(self.database);

        let result = distribution_service.execute_distribution(
            &source_account,
            &earnings_account,
            &tax_account,
            &reinvestment_account,
            profit,
            &rules,
            currency,
            Some(trade.id),
        )?;

        Ok(Some(result))
    }

    /// Calculate profit from a closed trade
    fn calculate_trade_profit(&self, trade: &Trade) -> Result<Decimal, Box<dyn Error>> {
        // Use the total_performance field which represents profit/loss
        Ok(trade.balance.total_performance)
    }

    fn find_distribution_accounts(
        &mut self,
        source_account_id: uuid::Uuid,
    ) -> Result<(Account, Account, Account), Box<dyn Error>> {
        let child_accounts: Vec<Account> = self
            .database
            .account_read()
            .all()?
            .into_iter()
            .filter(|account| account.parent_account_id == Some(source_account_id))
            .collect();

        let earnings_account = child_accounts
            .iter()
            .find(|acc| acc.account_type == AccountType::Earnings)
            .cloned()
            .ok_or("Earnings account not found")?;
        let tax_account = child_accounts
            .iter()
            .find(|acc| acc.account_type == AccountType::TaxReserve)
            .cloned()
            .ok_or("Tax reserve account not found")?;
        let reinvestment_account = child_accounts
            .iter()
            .find(|acc| acc.account_type == AccountType::Reinvestment)
            .cloned()
            .ok_or("Reinvestment account not found")?;

        Ok((earnings_account, tax_account, reinvestment_account))
    }
}

fn distribution_rules_or_none(
    result: Result<model::DistributionRules, Box<dyn Error>>,
) -> Result<Option<model::DistributionRules>, Box<dyn Error>> {
    match result {
        Ok(rules) => Ok(Some(rules)),
        Err(error) => {
            if error.downcast_ref::<DistributionRulesNotFound>().is_some() {
                return Ok(None);
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use db_sqlite::SqliteDatabase;
    use model::{Currency, DistributionRulesNotFound, Status};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn create_test_trade_profitable() -> Trade {
        use model::{Currency, TradeBalance};

        let mut trade = Trade {
            status: Status::ClosedTarget,
            ..Default::default()
        };

        // Create profitable balance
        let now = Utc::now().naive_utc();
        trade.balance = TradeBalance {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            currency: Currency::USD,
            funding: dec!(1000.0),            // Initial investment
            capital_in_market: dec!(0.0),     // No longer in market (closed)
            capital_out_market: dec!(1200.0), // Total capital out
            taxed: dec!(50.0),                // Tax amount
            total_performance: dec!(200.0),   // Profit: 1200 - 1000 = 200
        };

        trade
    }

    fn create_test_trade_loss() -> Trade {
        let mut trade = create_test_trade_profitable();
        trade.balance.capital_out_market = dec!(800.0); // Loss
        trade.balance.total_performance = dec!(-200.0); // Negative profit
        trade.status = Status::ClosedStopLoss;
        trade
    }

    fn create_sqlite_distribution_hierarchy(
        database: &SqliteDatabase,
    ) -> (Account, Account, Account, Account) {
        let source_account = database
            .account_write()
            .create_with_hierarchy(
                "event-main",
                "event main",
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
                "event-earnings",
                "event earnings",
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
                "event-tax",
                "event tax",
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
                "event-reinvestment",
                "event reinvestment",
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

    fn create_persisted_trade(database: &mut SqliteDatabase, account: &Account) -> Trade {
        let vehicle = database
            .trading_vehicle_write()
            .create_trading_vehicle(
                "EVENTDIST",
                Some("EVENTDIST"),
                &model::TradingVehicleCategory::Stock,
                "alpaca",
            )
            .expect("trading vehicle should be created");
        let stop = database
            .order_write()
            .create(
                &vehicle,
                10,
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
                10,
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
                10,
                dec!(120),
                &Currency::USD,
                &model::OrderAction::Sell,
                &model::OrderCategory::Limit,
            )
            .expect("target order should be created");
        let draft = model::DraftTrade {
            account: account.clone(),
            trading_vehicle: vehicle,
            quantity: 10,
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

    fn assert_transaction_total(transactions: &[model::Transaction], expected_total: Decimal) {
        assert_eq!(transactions.len(), 3);
        assert_eq!(
            transactions
                .iter()
                .try_fold(Decimal::ZERO, |total, tx| total.checked_add(tx.amount))
                .expect("transaction total should not overflow"),
            expected_total
        );
    }

    fn assert_single_transaction(
        transactions: &[model::Transaction],
        expected_category: model::TransactionCategory,
        expected_amount: Decimal,
        _label: &str,
    ) {
        assert_eq!(transactions.len(), 1);
        let transaction = transactions
            .first()
            .expect("destination transaction should exist");
        assert_eq!(transaction.category, expected_category);
        assert_eq!(transaction.amount, expected_amount);
    }

    #[test]
    fn test_calculate_trade_profit_profitable() {
        // Given: Event distribution service
        let mut database = SqliteDatabase::new_in_memory();
        let service = EventDistributionService::new(&mut database);

        // And: A profitable trade
        let trade = create_test_trade_profitable();

        // When: Calculate profit
        let profit = service.calculate_trade_profit(&trade).unwrap();

        // Then: Should return positive profit (1200 - 1000 = 200)
        assert_eq!(profit, dec!(200.0));
    }

    #[test]
    fn test_calculate_trade_profit_loss() {
        // Given: Event distribution service
        let mut database = SqliteDatabase::new_in_memory();
        let service = EventDistributionService::new(&mut database);

        // And: A losing trade
        let trade = create_test_trade_loss();

        // When: Calculate profit
        let profit = service.calculate_trade_profit(&trade).unwrap();

        // Then: Should return negative profit (800 - 1000 = -200)
        assert_eq!(profit, dec!(-200.0));
    }

    #[test]
    fn test_debug_redacts_database_trait_object() {
        let mut database = SqliteDatabase::new_in_memory();
        let service = EventDistributionService::new(&mut database);

        assert_eq!(
            format!("{service:?}"),
            "EventDistributionService { database: \"&mut dyn DatabaseFactory\" }"
        );
    }

    #[test]
    fn test_handle_trade_closed_event_loss_no_distribution() {
        // Given: Event distribution service
        let mut database = SqliteDatabase::new_in_memory();
        let mut service = EventDistributionService::new(&mut database);

        // And: A losing trade
        let trade = create_test_trade_loss();
        let currency = Currency::USD;

        // When: Handle trade closed event
        let result = service
            .handle_trade_closed_event(&trade, &currency)
            .unwrap();

        // Then: Should return None (no distribution for losses)
        assert!(result.is_none());
    }

    #[test]
    fn test_event_distribution_integration() {
        let mut database = SqliteDatabase::new_in_memory();
        let service = EventDistributionService::new(&mut database);
        let trade = create_test_trade_profitable();

        // The event service should identify profitable trades deterministically.
        let profit = service.calculate_trade_profit(&trade).unwrap();
        assert!(profit > Decimal::ZERO);
    }

    #[test]
    fn test_distribution_rules_or_none_propagates_distribution_read_errors() {
        let error = distribution_rules_or_none(Err("database unavailable".into()))
            .expect_err("non not-found errors should propagate");

        assert!(error.to_string().contains("database unavailable"));
    }

    #[test]
    fn test_distribution_rules_or_none_treats_rules_not_found_as_none() {
        let account_id = Uuid::new_v4();
        let result =
            distribution_rules_or_none(Err(DistributionRulesNotFound { account_id }.into()))
                .expect("missing distribution rules should opt out");

        assert!(result.is_none());
    }

    #[test]
    fn test_handle_trade_closed_event_treats_rules_not_found_as_none() {
        let mut database = SqliteDatabase::new_in_memory();
        let (source_account, _, _, _) = create_sqlite_distribution_hierarchy(&database);
        let mut trade = create_test_trade_profitable();
        trade.account_id = source_account.id;
        let mut service = EventDistributionService::new(&mut database);

        let result = service
            .handle_trade_closed_event(&trade, &Currency::USD)
            .expect("missing distribution rules should opt out");

        assert!(result.is_none());
    }

    #[test]
    fn test_find_distribution_accounts_returns_expected_children() {
        let mut database = SqliteDatabase::new_in_memory();
        let (source, earnings, tax, reinvestment) = create_sqlite_distribution_hierarchy(&database);
        let mut service = EventDistributionService::new(&mut database);

        let (e, t, r) = service
            .find_distribution_accounts(source.id)
            .expect("all distribution child accounts should exist");
        assert_eq!(e.id, earnings.id);
        assert_eq!(t.id, tax.id);
        assert_eq!(r.id, reinvestment.id);
    }

    #[test]
    fn test_find_distribution_accounts_errors_when_missing_required_child() {
        let mut database = SqliteDatabase::new_in_memory();
        let source = database
            .account_write()
            .create_with_hierarchy(
                "event-main",
                "event main",
                model::Environment::Paper,
                dec!(25),
                dec!(30),
                AccountType::Primary,
                None,
            )
            .expect("source account should be created");
        database
            .account_write()
            .create_with_hierarchy(
                "event-earnings",
                "event earnings",
                model::Environment::Paper,
                dec!(0),
                dec!(0),
                AccountType::Earnings,
                Some(source.id),
            )
            .expect("earnings account should be created");
        database
            .account_write()
            .create_with_hierarchy(
                "event-tax",
                "event tax",
                model::Environment::Paper,
                dec!(0),
                dec!(0),
                AccountType::TaxReserve,
                Some(source.id),
            )
            .expect("tax account should be created");

        let mut service = EventDistributionService::new(&mut database);

        let err = service
            .find_distribution_accounts(source.id)
            .expect_err("missing reinvestment account should fail");
        assert!(err.to_string().contains("Reinvestment account not found"));
    }

    #[test]
    fn test_handle_trade_closed_event_returns_none_when_below_threshold() {
        let mut database = SqliteDatabase::new_in_memory();
        let (source_account, _, _, _) = create_sqlite_distribution_hierarchy(&database);
        database
            .distribution_write()
            .create_or_update(
                source_account.id,
                dec!(0.40),
                dec!(0.30),
                dec!(0.30),
                dec!(1_000_000),
                "hashed-password",
            )
            .expect("distribution rules should be stored");
        let mut trade = create_test_trade_profitable();
        trade.account_id = source_account.id;
        let mut service = EventDistributionService::new(&mut database);

        let result = service
            .handle_trade_closed_event(&trade, &Currency::USD)
            .expect("below threshold should short-circuit");
        assert!(result.is_none());
    }

    #[test]
    fn test_handle_trade_closed_event_propagates_account_lookup_errors() {
        let mut database = SqliteDatabase::new_in_memory();
        let trade = create_test_trade_profitable();
        let mut service = EventDistributionService::new(&mut database);

        let err = service
            .handle_trade_closed_event(&trade, &Currency::USD)
            .expect_err("account lookup errors should propagate");
        assert!(!err.to_string().is_empty());
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_handle_trade_closed_event_executes_distribution_and_audit_trail() {
        let mut database = SqliteDatabase::new_in_memory();
        let (source_account, earnings_account, tax_account, reinvestment_account) =
            create_sqlite_distribution_hierarchy(&database);
        database
            .distribution_write()
            .create_or_update(
                source_account.id,
                dec!(0.40),
                dec!(0.30),
                dec!(0.30),
                dec!(100),
                "hashed-password",
            )
            .expect("distribution rules should be stored");

        let mut trade = create_persisted_trade(&mut database, &source_account);
        trade.status = Status::ClosedTarget;
        trade.balance.total_performance = dec!(1000);

        let distribution_result = {
            let mut service = EventDistributionService::new(&mut database);
            service
                .handle_trade_closed_event(&trade, &Currency::USD)
                .expect("profitable trade event should be handled")
                .expect("profitable trade should distribute")
        };

        assert_eq!(distribution_result.source_account_id, source_account.id);
        assert_eq!(distribution_result.original_amount, dec!(1000));
        assert_eq!(distribution_result.earnings_amount, Some(dec!(400.00)));
        assert_eq!(distribution_result.tax_amount, Some(dec!(300.00)));
        assert_eq!(distribution_result.reinvestment_amount, Some(dec!(300.00)));
        assert_eq!(distribution_result.transactions_created.len(), 3);

        let history = database
            .distribution_read()
            .history_for_account(source_account.id)
            .expect("distribution history should be readable");
        assert_eq!(history.len(), 1);
        let history = history.first().expect("history row should exist");
        assert_eq!(history.trade_id, Some(trade.id));
        assert_eq!(history.original_amount, dec!(1000));

        let source_transactions =
            account_transactions(&database, source_account.id, &Currency::USD, "source");
        assert_transaction_total(&source_transactions, dec!(-1000));

        let earnings_transactions =
            account_transactions(&database, earnings_account.id, &Currency::USD, "earnings");
        let tax_transactions =
            account_transactions(&database, tax_account.id, &Currency::USD, "tax");
        let reinvestment_transactions = account_transactions(
            &database,
            reinvestment_account.id,
            &Currency::USD,
            "reinvestment",
        );

        assert_single_transaction(
            &earnings_transactions,
            model::TransactionCategory::PaymentEarnings(trade.id),
            dec!(400),
            "earnings",
        );
        assert_single_transaction(
            &tax_transactions,
            model::TransactionCategory::PaymentTax(trade.id),
            dec!(300),
            "tax",
        );
        assert_single_transaction(
            &reinvestment_transactions,
            model::TransactionCategory::Deposit,
            dec!(300),
            "reinvestment",
        );
    }
}
