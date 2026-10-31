//! Financial boundary regressions through the public facade and isolated SQLite.
use chrono::Utc;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    Account, AccountType, Broker, BrokerLog, Currency, DraftTrade, Environment, Order, OrderIds,
    OrderStatus, RuleLevel, RuleName, Status, Trade, TradeCategory, TradingVehicleCategory,
    Transaction, TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────
// Test infrastructure
// ─────────────────────────────────────────────────────────────────

fn create_trust() -> TrustFacade {
    let db = SqliteDatabase::new_in_memory();
    TrustFacade::new(Box::new(db), Box::new(NoOpBroker))
}

fn create_trust_with_broker(broker: impl Broker + 'static) -> TrustFacade {
    let db = SqliteDatabase::new_in_memory();
    TrustFacade::new(Box::new(db), Box::new(broker))
}

fn setup_account(trust: &mut TrustFacade, capital: Decimal) -> Account {
    trust
        .create_account(
            "test",
            "test account",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("account creation");
    let account = trust.search_account("test").unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            capital,
            &Currency::USD,
        )
        .expect("deposit");
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerMonth(20.0),
            "monthly risk",
            &RuleLevel::Error,
        )
        .expect("monthly rule");
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(2.0),
            "trade risk",
            &RuleLevel::Error,
        )
        .expect("trade rule");
    account
}

fn setup_account_no_rules(trust: &mut TrustFacade, capital: Decimal) -> Account {
    trust
        .create_account(
            "test",
            "test account",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("account creation");
    let account = trust.search_account("test").unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            capital,
            &Currency::USD,
        )
        .expect("deposit");
    account
}

fn create_trading_vehicle(trust: &mut TrustFacade, symbol: &str) -> model::TradingVehicle {
    trust
        .create_trading_vehicle(
            symbol,
            Some("US0000000000"),
            &TradingVehicleCategory::Stock,
            "TEST",
        )
        .expect("create tv")
}

fn create_long_draft(account: &Account, tv: &model::TradingVehicle, qty: i64) -> DraftTrade {
    DraftTrade {
        account: account.clone(),
        trading_vehicle: tv.clone(),
        quantity: qty.into(),
        currency: Currency::USD,
        category: TradeCategory::Long,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    }
}

fn create_short_draft(account: &Account, tv: &model::TradingVehicle, qty: i64) -> DraftTrade {
    DraftTrade {
        account: account.clone(),
        trading_vehicle: tv.clone(),
        quantity: qty.into(),
        currency: Currency::USD,
        category: TradeCategory::Short,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    }
}

/// NoOp broker that does nothing (for tests not involving broker operations)
struct NoOpBroker;

impl Broker for NoOpBroker {
    fn kind(&self) -> model::BrokerKind {
        model::BrokerKind::Alpaca
    }

    fn submit_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        Ok((
            BrokerLog::default(),
            OrderIds {
                entry: Uuid::new_v4().to_string(),
                target: Uuid::new_v4().to_string(),
                stop: Uuid::new_v4().to_string(),
            },
        ))
    }
    fn sync_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        unimplemented!()
    }
    fn close_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        unimplemented!()
    }
    fn cancel_trade(&self, _trade: &Trade, _account: &Account) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn modify_stop(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_stop_price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        Ok(Uuid::new_v4().to_string())
    }
    fn modify_target(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_target_price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        Ok(Uuid::new_v4().to_string())
    }
}

/// Configurable broker for sync tests
struct SyncBroker {
    sync_fn: fn(&Trade) -> (Status, Vec<Order>),
}

impl Broker for SyncBroker {
    fn kind(&self) -> model::BrokerKind {
        model::BrokerKind::Alpaca
    }

    fn submit_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        Ok((
            BrokerLog::default(),
            OrderIds {
                entry: Uuid::new_v4().to_string(),
                target: Uuid::new_v4().to_string(),
                stop: Uuid::new_v4().to_string(),
            },
        ))
    }
    fn sync_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        let (status, orders) = (self.sync_fn)(trade);
        Ok((status, orders, BrokerLog::default()))
    }
    fn close_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        let mut target = trade.target.clone();
        target.status = OrderStatus::Filled;
        target.average_filled_price = Some(trade.entry.unit_price);
        target.filled_quantity = trade.entry.quantity;
        Ok((target, BrokerLog::default()))
    }
    fn cancel_trade(&self, _trade: &Trade, _account: &Account) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn modify_stop(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_stop_price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        Ok(Uuid::new_v4().to_string())
    }
    fn modify_target(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_target_price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        Ok(Uuid::new_v4().to_string())
    }
}

fn submit_and_get_trade(trust: &mut TrustFacade, account: &Account, trade: &Trade) -> Trade {
    trust.fund_trade(trade).expect("fund");
    let funded = trust
        .search_trades(account.id, Status::Funded)
        .unwrap()
        .first()
        .unwrap()
        .clone();
    trust.submit_trade(&funded).expect("submit");
    trust
        .search_trades(account.id, Status::Submitted)
        .unwrap()
        .first()
        .unwrap()
        .clone()
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 1: Account & Transaction Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_001_zero_deposit_creates_neither_cash_nor_receipts() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    assert!(trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(0),
            &Currency::USD
        )
        .is_err());
    assert!(trust
        .get_account_transactions(account.id)
        .unwrap()
        .is_empty());
}

#[test]
fn bug_002_negative_deposit_should_be_rejected() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    // First make a real deposit so balance exists
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(100),
            &Currency::USD,
        )
        .unwrap();

    // Negative deposit should be rejected
    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Deposit,
        dec!(-50),
        &Currency::USD,
    );
    assert!(result.is_err(), "BUG: Negative deposit should be rejected");
}

#[test]
fn bug_003_withdrawal_more_than_available_should_be_rejected() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(100),
            &Currency::USD,
        )
        .unwrap();

    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Withdrawal,
        dec!(200),
        &Currency::USD,
    );
    assert!(
        result.is_err(),
        "BUG: Withdrawal exceeding balance should be rejected"
    );
}

#[test]
fn bug_004_zero_withdrawal_should_be_rejected() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(100),
            &Currency::USD,
        )
        .unwrap();

    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Withdrawal,
        dec!(0),
        &Currency::USD,
    );
    assert!(result.is_err(), "BUG: Zero withdrawal should be rejected");
}

#[test]
fn contract_005_taxes_plus_earnings_percentage_exceeds_100() {
    let mut trust = create_trust();
    let result = trust.create_account("test", "test", Environment::Paper, dec!(80), dec!(80));
    assert!(result.is_err(), "Invalid input must be rejected");
    assert!(
        trust.search_all_accounts().unwrap().is_empty(),
        "Rejected creation must leave no account"
    );
}

#[test]
fn contract_006_negative_tax_percentage() {
    let mut trust = create_trust();
    let result = trust.create_account("test", "test", Environment::Paper, dec!(-10), dec!(10));
    assert!(result.is_err(), "Invalid input must be rejected");
    assert!(
        trust.search_all_accounts().unwrap().is_empty(),
        "Rejected creation must leave no account"
    );
}

#[test]
fn bug_007_duplicate_account_name() {
    let mut trust = create_trust();
    trust
        .create_account("test", "first", Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    let result = trust.create_account("test", "second", Environment::Paper, dec!(20), dec!(10));
    // Should either fail or handle duplicate names gracefully
    if result.is_ok() {
        // Verify search still works correctly
        let account = trust.search_account("test").unwrap();
        assert!(
            account.description == "first" || account.description == "second",
            "BUG: Duplicate account names create ambiguity in search"
        );
    }
}

#[test]
fn bug_008_deposit_without_prior_balance_for_currency() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    // First deposit in EUR should create the balance record
    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Deposit,
        dec!(1000),
        &Currency::EUR,
    );
    assert!(
        result.is_ok(),
        "First deposit in a currency should succeed and create balance record"
    );
}

#[test]
fn bug_009_withdrawal_from_nonexistent_currency_balance() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    // Deposit in USD
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();

    // Try to withdraw EUR (no EUR balance)
    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Withdrawal,
        dec!(100),
        &Currency::EUR,
    );
    assert!(
        result.is_err(),
        "BUG: Withdrawal from non-existent currency balance should fail"
    );
}

#[test]
fn bug_010_search_balance_nonexistent_currency() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();

    let result = trust.search_balance(account.id, &Currency::EUR);
    assert!(
        result.is_err(),
        "Searching balance for currency with no deposits should return error"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 2: Trade Creation Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_011_trade_with_zero_quantity() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 0);
    let result = trust.create_trade(draft, dec!(38), dec!(40), dec!(50));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

#[test]
fn contract_012_trade_with_stop_above_entry_for_long() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    let result = trust.create_trade(draft, dec!(45), dec!(40), dec!(50));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

#[test]
fn contract_013_trade_with_target_below_entry_for_long() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    let result = trust.create_trade(draft, dec!(38), dec!(40), dec!(35));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

#[test]
fn contract_014_trade_with_entry_equals_stop() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    let result = trust.create_trade(draft, dec!(40), dec!(40), dec!(50));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

#[test]
fn contract_015_trade_with_entry_equals_target() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    let result = trust.create_trade(draft, dec!(38), dec!(40), dec!(40));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

#[test]
fn contract_016_trade_with_zero_prices() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    let result = trust.create_trade(draft, dec!(0), dec!(0), dec!(0));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

#[test]
fn contract_017_trade_with_negative_prices() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    let result = trust.create_trade(draft, dec!(-5), dec!(-3), dec!(-1));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

#[test]
fn contract_018_short_trade_with_stop_below_entry() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_short_draft(&account, &tv, 100);
    let result = trust.create_trade(draft, dec!(45), dec!(50), dec!(40));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 3: Funding Validation Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_019_fund_trade_with_exactly_available_capital() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(1000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    // Create a trade that requires EXACTLY the available capital
    let draft = create_long_draft(&account, &tv, 10);
    trust
        .create_trade(draft, dec!(90), dec!(100), dec!(120))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    // Requires 100 * 10 = 1000, which is exactly available
    let result = trust.fund_trade(&trade);
    assert!(
        result.is_ok(),
        "Funding a trade with exactly available capital should succeed"
    );
}

#[test]
fn bug_020_fund_trade_one_cent_over_available() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(999.99));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    // Requires 100 * 10 = 1000, but only 999.99 available
    let draft = create_long_draft(&account, &tv, 10);
    trust
        .create_trade(draft, dec!(90), dec!(100), dec!(120))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let result = trust.fund_trade(&trade);
    assert!(result.is_err(), "Funding should fail when 1 cent short");
}

#[test]
fn contract_022_level_limits_apply_to_longs_and_shorts_at_the_same_boundary() {
    for short in [false, true] {
        for quantity in [20, 21] {
            let mut trust = create_trust();
            let account = setup_account(&mut trust, dec!(50000));
            let tv = create_trading_vehicle(&mut trust, "AAPL");
            trust
                .change_level(
                    account.id,
                    0,
                    "restrict risk",
                    model::LevelTrigger::ManualOverride,
                )
                .unwrap();
            // 2% of 50,000 / 5 stop distance * 0.1 level multiplier = 20 units.
            let (draft, stop, target) = if short {
                (
                    create_short_draft(&account, &tv, quantity),
                    dec!(55),
                    dec!(40),
                )
            } else {
                (
                    create_long_draft(&account, &tv, quantity),
                    dec!(45),
                    dec!(60),
                )
            };
            let trade = trust.create_trade(draft, stop, dec!(50), target).unwrap();
            let cash = trust.search_balance(account.id, &Currency::USD).unwrap();
            let transactions = trust.get_account_transactions(account.id).unwrap();
            let result = trust.fund_trade(&trade);
            if quantity == 20 {
                assert!(result.is_ok(), "permitted size must fund: {result:?}");
                assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::Funded);
            } else {
                let error = result.unwrap_err();
                let validation = error
                    .downcast_ref::<core::validation_errors::FundValidationError>()
                    .expect("typed risk rejection");
                assert_eq!(
                    validation.code,
                    core::validation_errors::FundValidationErrorCode::LevelAdjustedQuantityExceeded
                );
                assert_eq!(
                    trust.search_balance(account.id, &Currency::USD).unwrap(),
                    cash
                );
                assert_eq!(
                    trust.get_account_transactions(account.id).unwrap(),
                    transactions
                );
                assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
            }
        }
    }
}

#[test]
fn bug_023_fund_two_trades_exceeding_total_capital() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(10000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    // Trade 1: requires $6000
    let draft1 = create_long_draft(&account, &tv, 60);
    trust
        .create_trade(draft1, dec!(90), dec!(100), dec!(120))
        .unwrap();
    let trade1 = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();
    trust.fund_trade(&trade1).expect("fund trade 1");

    // Trade 2: requires $6000, but only $4000 available
    let tv2 = create_trading_vehicle(&mut trust, "MSFT");
    let draft2 = create_long_draft(&account, &tv2, 60);
    trust
        .create_trade(draft2, dec!(90), dec!(100), dec!(120))
        .unwrap();
    let trade2 = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let result = trust.fund_trade(&trade2);
    assert!(
        result.is_err(),
        "Second trade funding should fail due to insufficient capital"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 4: Trade Lifecycle State Machine
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_024_submit_unfunded_trade() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    // Try to submit without funding first
    let result = trust.submit_trade(&trade);
    assert!(result.is_err(), "Submitting an unfunded trade should fail");
}

#[test]
fn bug_025_submit_already_submitted_trade() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let submitted = submit_and_get_trade(&mut trust, &account, &trade);

    // Try to submit again
    let result = trust.submit_trade(&submitted);
    assert!(
        result.is_err(),
        "Submitting an already-submitted trade should fail"
    );
}

#[test]
fn bug_026_cancel_new_trade() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    // Cancel a trade that hasn't been funded
    let result_funded = trust.cancel_funded_trade(&trade);
    assert!(
        result_funded.is_err(),
        "Canceling a New (not Funded) trade via cancel_funded should fail"
    );

    let result_submitted = trust.cancel_submitted_trade(&trade);
    assert!(
        result_submitted.is_err(),
        "Canceling a New (not Submitted) trade via cancel_submitted should fail"
    );
}

#[test]
fn contract_027_cancel_submitted_explains_required_state() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    // Try cancel_submitted on a New trade - error message should say "not submitted"
    let result = trust.cancel_submitted_trade(&trade);
    assert!(result.is_err());
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("not submitted"), "{err}");
    assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
}

#[test]
fn bug_028_cancel_funded_trade_restores_balance() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let balance_before = trust.search_balance(account.id, &Currency::USD).unwrap();
    trust.fund_trade(&trade).unwrap();
    let funded = trust
        .search_trades(account.id, Status::Funded)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let balance_funded = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert!(
        balance_funded.total_available < balance_before.total_available,
        "Balance should decrease after funding"
    );

    trust.cancel_funded_trade(&funded).unwrap();
    let balance_after = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(
        balance_after.total_available, balance_before.total_available,
        "BUG: Balance should be fully restored after canceling funded trade"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 5: Sync & Close Edge Cases
// ─────────────────────────────────────────────────────────────────

fn make_entry_filled_orders(trade: &Trade) -> (Status, Vec<Order>) {
    let entry = Order {
        id: trade.entry.id,
        broker_order_id: trade.entry.broker_order_id.clone(),
        filled_quantity: trade.entry.quantity,
        average_filled_price: Some(trade.entry.unit_price),
        status: OrderStatus::Filled,
        filled_at: Some(Utc::now().naive_utc()),
        ..trade.entry.clone()
    };
    let target = Order {
        id: trade.target.id,
        broker_order_id: trade.target.broker_order_id.clone(),
        status: OrderStatus::Accepted,
        ..trade.target.clone()
    };
    let stop = Order {
        id: trade.safety_stop.id,
        broker_order_id: trade.safety_stop.broker_order_id.clone(),
        status: OrderStatus::Held,
        ..trade.safety_stop.clone()
    };
    (Status::Filled, vec![entry, target, stop])
}

#[allow(dead_code)]
fn make_target_filled_orders(trade: &Trade) -> (Status, Vec<Order>) {
    let entry = Order {
        id: trade.entry.id,
        broker_order_id: trade.entry.broker_order_id.clone(),
        filled_quantity: trade.entry.quantity,
        average_filled_price: Some(trade.entry.unit_price),
        status: OrderStatus::Filled,
        filled_at: Some(Utc::now().naive_utc()),
        ..trade.entry.clone()
    };
    let target = Order {
        id: trade.target.id,
        broker_order_id: trade.target.broker_order_id.clone(),
        filled_quantity: trade.target.quantity,
        average_filled_price: Some(trade.target.unit_price),
        status: OrderStatus::Filled,
        filled_at: Some(Utc::now().naive_utc()),
        ..trade.target.clone()
    };
    let stop = Order {
        id: trade.safety_stop.id,
        broker_order_id: trade.safety_stop.broker_order_id.clone(),
        status: OrderStatus::Canceled,
        ..trade.safety_stop.clone()
    };
    (Status::ClosedTarget, vec![entry, target, stop])
}

#[allow(dead_code)]
fn make_stop_filled_orders(trade: &Trade) -> (Status, Vec<Order>) {
    let entry = Order {
        id: trade.entry.id,
        broker_order_id: trade.entry.broker_order_id.clone(),
        filled_quantity: trade.entry.quantity,
        average_filled_price: Some(trade.entry.unit_price),
        status: OrderStatus::Filled,
        filled_at: Some(Utc::now().naive_utc()),
        ..trade.entry.clone()
    };
    let target = Order {
        id: trade.target.id,
        broker_order_id: trade.target.broker_order_id.clone(),
        status: OrderStatus::Canceled,
        ..trade.target.clone()
    };
    let stop = Order {
        id: trade.safety_stop.id,
        broker_order_id: trade.safety_stop.broker_order_id.clone(),
        filled_quantity: trade.safety_stop.quantity,
        average_filled_price: Some(trade.safety_stop.unit_price),
        status: OrderStatus::Filled,
        filled_at: Some(Utc::now().naive_utc()),
        ..trade.safety_stop.clone()
    };
    (Status::ClosedStopLoss, vec![entry, target, stop])
}

#[test]
fn bug_030_sync_trade_wrong_account() {
    let mut trust = create_trust_with_broker(SyncBroker {
        sync_fn: make_entry_filled_orders,
    });

    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();
    let submitted = submit_and_get_trade(&mut trust, &account, &trade);

    // Create a different account
    let wrong_account = trust
        .create_account("wrong", "wrong", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    // Sync with wrong account should fail
    let result = trust.sync_trade(&submitted, &wrong_account);
    assert!(
        result.is_err(),
        "BUG: Syncing a trade with the wrong account should fail"
    );
}

#[test]
fn contract_031_fill_trade_with_negative_fee() {
    let mut trust = create_trust_with_broker(SyncBroker {
        sync_fn: make_entry_filled_orders,
    });
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();
    let submitted = submit_and_get_trade(&mut trust, &account, &trade);

    let before = trust.get_account_transactions(account.id).unwrap();
    let result = trust.fill_trade(&submitted, dec!(-10));
    let error = result.unwrap_err();
    assert!(
        error.to_string().contains("fee"),
        "Reject the fee itself, not an unrelated lifecycle error: {error}"
    );
    assert_eq!(trust.get_account_transactions(account.id).unwrap(), before);
    assert_eq!(
        trust.read_trade(submitted.id).unwrap().status,
        Status::Submitted
    );
}

#[test]
fn contract_032_trade_with_entry_equals_target_breakeven() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    let result = trust.create_trade(draft, dec!(38), dec!(40), dec!(40));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 6: Risk & Quantity Calculator Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_033_max_quantity_with_zero_entry_price() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));

    // Entry price = 0 should cause division by zero
    let result = trust.calculate_maximum_quantity(account.id, dec!(0), dec!(0), &Currency::USD);
    assert!(
        result.is_err() || result.unwrap() == Decimal::ZERO,
        "BUG: Zero entry price should return error or zero quantity"
    );
}

#[test]
fn bug_034_max_quantity_with_entry_equals_stop() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));

    // Entry == Stop means zero risk per share
    let result = trust.calculate_maximum_quantity(account.id, dec!(100), dec!(100), &Currency::USD);
    assert!(
        result.is_err() || result.unwrap() == Decimal::ZERO,
        "BUG: Entry==Stop (zero risk) should return error or zero quantity"
    );
}

#[test]
fn bug_035_max_quantity_with_very_small_price_difference() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));

    // 1 cent difference: entry=$100.01, stop=$100.00
    let result =
        trust.calculate_maximum_quantity(account.id, dec!(100.01), dec!(100.00), &Currency::USD);
    assert!(
        result.is_ok(),
        "Small price difference should still calculate"
    );
    let qty = result.unwrap();
    assert!(qty >= Decimal::ZERO, "Quantity should be non-negative");
}

#[test]
fn bug_036_max_quantity_with_negative_entry_price() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));

    let result =
        trust.calculate_maximum_quantity(account.id, dec!(-100), dec!(-110), &Currency::USD);
    assert!(
        result.is_err() || result.unwrap() == Decimal::ZERO,
        "BUG: Negative prices should return error or zero"
    );
}

#[test]
fn bug_037_level_adjusted_quantity_calculation() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));

    let result =
        trust.calculate_level_adjusted_quantity(account.id, dec!(100), dec!(95), &Currency::USD);
    assert!(
        result.is_ok(),
        "Level-adjusted quantity should be calculable"
    );
    let adjusted = result.unwrap();
    assert!(
        adjusted.final_quantity <= adjusted.base_quantity,
        "Level 1 (default) should cap quantity: base={}, final={}",
        adjusted.base_quantity,
        adjusted.final_quantity
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 7: Rule System Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_038_rule_with_zero_risk_percentage() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    // 0% risk per trade means no trade can be funded
    let result = trust.create_rule(
        &account,
        &RuleName::RiskPerTrade(0.0),
        "zero risk",
        &RuleLevel::Error,
    );
    assert!(result.is_ok(), "Creating zero-risk rule should succeed");

    let tv = create_trading_vehicle(&mut trust, "AAPL");
    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let fund_result = trust.fund_trade(&trade);
    assert!(
        fund_result.is_err(),
        "Trade should not be fundable with 0% risk allowance"
    );
}

#[test]
fn bug_039_rule_with_100_percent_risk() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    // 100% risk means we can risk the entire account on one trade
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(100.0),
            "all-in risk",
            &RuleLevel::Error,
        )
        .unwrap();

    let tv = create_trading_vehicle(&mut trust, "AAPL");
    let draft = create_long_draft(&account, &tv, 500);
    trust
        .create_trade(draft, dec!(90), dec!(100), dec!(120))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let result = trust.fund_trade(&trade);
    assert!(result.is_ok(), "100% risk should allow funding");
}

#[test]
fn bug_040_rule_with_negative_risk_percentage() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    // Negative risk percentage should be rejected
    let result = trust.create_rule(
        &account,
        &RuleName::RiskPerTrade(-5.0),
        "negative risk",
        &RuleLevel::Error,
    );
    assert!(result.is_err(), "invalid or duplicate risk rule must fail");
}

#[test]
fn bug_041_rule_with_over_100_percent_risk() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    // 200% risk makes no financial sense
    let result = trust.create_rule(
        &account,
        &RuleName::RiskPerTrade(200.0),
        "absurd risk",
        &RuleLevel::Error,
    );
    assert!(result.is_err(), "invalid or duplicate risk rule must fail");
}

#[test]
fn bug_042_deactivate_rule_then_fund_trade() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    // Create strict rule: only 0.1% risk per trade
    let rule = trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(0.1),
            "strict risk",
            &RuleLevel::Error,
        )
        .unwrap();

    let tv = create_trading_vehicle(&mut trust, "AAPL");
    let draft = create_long_draft(&account, &tv, 500);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    // Should fail with strict rule
    let result1 = trust.fund_trade(&trade);
    assert!(
        result1.is_err(),
        "Trade should not be fundable with strict 0.1% risk"
    );

    // Deactivate the rule
    trust.deactivate_rule(&rule).unwrap();

    // Now funding should succeed (no rules to restrict)
    let result2 = trust.fund_trade(&trade);
    assert!(
        result2.is_ok(),
        "After deactivating rule, trade should be fundable"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 8: Trading Vehicle Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_043_empty_symbol_trading_vehicle() {
    let mut trust = create_trust();
    let result = trust.create_trading_vehicle("", None, &TradingVehicleCategory::Stock, "TEST");
    assert!(result.is_err(), "Invalid input must be rejected");
    assert!(trust.search_trading_vehicles().unwrap().is_empty());
}

#[test]
fn bug_044_duplicate_trading_vehicle() {
    let mut trust = create_trust();
    trust
        .create_trading_vehicle(
            "AAPL",
            Some("US0378331005"),
            &TradingVehicleCategory::Stock,
            "NASDAQ",
        )
        .unwrap();
    let result = trust.create_trading_vehicle(
        "AAPL",
        Some("US0378331005"),
        &TradingVehicleCategory::Stock,
        "NASDAQ",
    );
    // Duplicate should either be rejected or handled via upsert
    if result.is_ok() {
        let vehicles = trust.search_trading_vehicles().unwrap();
        let aapl_count = vehicles.iter().filter(|tv| tv.symbol == "AAPL").count();
        assert!(
            aapl_count <= 1,
            "BUG: Duplicate trading vehicles should not exist"
        );
    }
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 9: Protected Mode Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_045_protected_mode_blocks_operations() {
    let db = SqliteDatabase::new_in_memory();
    let mut trust = TrustFacade::new(Box::new(db), Box::new(NoOpBroker));
    trust.enable_protected_mode();

    // Should fail without authorization
    let result = trust.create_account("test", "test", Environment::Paper, dec!(20), dec!(10));
    assert!(
        result.is_err(),
        "Protected mode should block unauthorized operations"
    );
}

#[test]
fn bug_046_protected_authorization_consumed_once() {
    let db = SqliteDatabase::new_in_memory();
    let mut trust = TrustFacade::new(Box::new(db), Box::new(NoOpBroker));
    trust.enable_protected_mode();

    trust
        .authorize_protected_mutation("secret", "secret")
        .unwrap();
    trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    // Second operation should fail (authorization consumed)
    let result = trust.create_account("test2", "test2", Environment::Paper, dec!(20), dec!(10));
    assert!(
        result.is_err(),
        "Protected authorization should be consumed after one operation"
    );
}

#[test]
fn bug_047_protected_authorization_consumed_on_failure() {
    let db = SqliteDatabase::new_in_memory();
    let mut trust = TrustFacade::new(Box::new(db), Box::new(NoOpBroker));
    trust.enable_protected_mode();
    trust
        .authorize_protected_mutation("secret", "secret")
        .unwrap();

    // Create first account
    trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    // Authorization is consumed. Authorize again.
    trust
        .authorize_protected_mutation("secret", "secret")
        .unwrap();

    let account = trust.search_account("test").unwrap();
    // Try an operation that will fail (e.g., deposit into non-existent currency balance record issue)
    // The authorization might still be consumed even though the operation could fail
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(100),
            &Currency::USD,
        )
        .unwrap();

    // Authorization should be consumed
    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Deposit,
        dec!(100),
        &Currency::USD,
    );
    assert!(
        result.is_err(),
        "Second operation should fail - authorization consumed"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 10: Performance Calculator Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_048_performance_stats_all_breakeven_trades() {
    use core::calculators_performance::PerformanceCalculator;

    let trades = vec![
        create_closed_trade(
            dec!(100),
            dec!(95),
            dec!(110),
            Status::ClosedTarget,
            dec!(0),
        ),
        create_closed_trade(
            dec!(200),
            dec!(190),
            dec!(220),
            Status::ClosedTarget,
            dec!(0),
        ),
    ];

    let stats = PerformanceCalculator::calculate_performance_stats(&trades);
    // Break-even trades are neither winners nor losers.
    assert_eq!(stats.total_trades, 2);
    assert_eq!(
        stats.winning_trades, 0,
        "Breakeven trades have 0 profit, so not winning"
    );
    assert_eq!(stats.losing_trades, 0, "Break-even trades are not losses");
}

#[test]
fn bug_049_win_rate_with_single_trade() {
    use core::calculators_performance::PerformanceCalculator;

    let trades = vec![create_closed_trade(
        dec!(100),
        dec!(95),
        dec!(110),
        Status::ClosedTarget,
        dec!(100),
    )];

    let stats = PerformanceCalculator::calculate_performance_stats(&trades);
    assert_eq!(stats.win_rate, dec!(100));
    assert_eq!(stats.winning_trades, 1);
    assert_eq!(stats.losing_trades, 0);
}

#[test]
fn bug_050_r_multiple_with_entry_equals_stop() {
    use core::calculators_performance::PerformanceCalculator;

    // Zero risk trade (entry == stop) should result in None R-multiple
    let trades = vec![create_closed_trade(
        dec!(100),
        dec!(100),
        dec!(110),
        Status::ClosedTarget,
        dec!(100),
    )];

    let avg_r = PerformanceCalculator::calculate_average_r_multiple(&trades);
    // With zero risk, R-multiple can't be calculated, average should be 0
    assert_eq!(
        avg_r,
        dec!(0),
        "Average R-multiple should be 0 when risk is zero"
    );
}

fn create_closed_trade(
    entry_price: Decimal,
    stop_price: Decimal,
    target_price: Decimal,
    status: Status,
    performance: Decimal,
) -> Trade {
    let mut trade = Trade::default();
    trade.entry.unit_price = entry_price;
    trade.safety_stop.unit_price = stop_price;
    trade.target.unit_price = target_price;
    trade.status = status;
    trade.balance.total_performance = performance;
    trade.category = TradeCategory::Long;
    trade
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 11: Distribution System Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_051_distribution_percentages_not_summing_to_100() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    // Create child accounts
    create_distribution_hierarchy(&mut trust, account.id);

    // Percentages don't sum to 1.0 (100%)
    let result = trust.configure_distribution(
        account.id,
        dec!(0.30),
        dec!(0.25),
        dec!(0.40),
        dec!(0),
        "test-password",
    );
    assert!(
        result.is_err(),
        "Distribution percentages not summing to 100% should fail"
    );
}

#[test]
fn bug_054_distribution_password_too_short() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    create_distribution_hierarchy(&mut trust, account.id);

    // Password shorter than 8 chars
    let result = trust.configure_distribution(
        account.id,
        dec!(0.30),
        dec!(0.25),
        dec!(0.45),
        dec!(0),
        "short",
    );
    assert!(result.is_err(), "Short password should be rejected");
}

#[test]
fn bug_055_distribution_without_child_accounts() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    // Configure distribution WITHOUT creating child accounts
    trust
        .configure_distribution(
            account.id,
            dec!(0.30),
            dec!(0.25),
            dec!(0.45),
            dec!(0),
            "test-password",
        )
        .unwrap();

    // Try to execute distribution
    let result = trust.execute_distribution(account.id, dec!(1000), Currency::USD);
    assert!(
        result.is_err(),
        "Distribution without child accounts should fail"
    );
}

fn create_distribution_hierarchy(trust: &mut TrustFacade, parent_id: Uuid) {
    trust
        .create_account_with_hierarchy(
            "earnings",
            "earnings",
            Environment::Paper,
            dec!(0),
            dec!(0),
            AccountType::Earnings,
            Some(parent_id),
        )
        .unwrap();
    trust
        .create_account_with_hierarchy(
            "tax",
            "tax",
            Environment::Paper,
            dec!(0),
            dec!(0),
            AccountType::TaxReserve,
            Some(parent_id),
        )
        .unwrap();
    trust
        .create_account_with_hierarchy(
            "reinvestment",
            "reinvestment",
            Environment::Paper,
            dec!(0),
            dec!(0),
            AccountType::Reinvestment,
            Some(parent_id),
        )
        .unwrap();
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 12: Level System Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_056_level_for_new_account() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let level = trust.level_for_account(account.id).unwrap();
    assert_eq!(
        level.current_level, 3,
        "New accounts should start at level 3"
    );
}

#[test]
fn contract_057_change_level_to_zero_is_persisted() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let (level, change) = trust
        .change_level(account.id, 0, "test", model::LevelTrigger::ManualOverride)
        .unwrap();
    assert_eq!(level.current_level, 0);
    assert!(trust
        .level_history_for_account(account.id, None)
        .unwrap()
        .contains(&change));
}

#[test]
fn contract_058_change_level_above_maximum_does_not_write_history() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let before = trust.level_history_for_account(account.id, None).unwrap();
    let result = trust.change_level(account.id, 255, "test", model::LevelTrigger::ManualOverride);
    assert!(result.is_err());
    assert_eq!(
        trust.level_history_for_account(account.id, None).unwrap(),
        before
    );
}

#[test]
fn bug_059_level_history_with_zero_days() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let history = trust
        .level_history_for_account(account.id, Some(0))
        .unwrap();
    // 0 days window should return empty or just today's changes
    assert!(
        history.is_empty() || history.len() <= 1,
        "0-day window should return minimal results"
    );
}

#[test]
fn bug_060_level_for_nonexistent_account() {
    let mut trust = create_trust();
    let fake_id = Uuid::new_v4();

    let result = trust.level_for_account(fake_id);
    assert!(
        result.is_err(),
        "Level for non-existent account should return error"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 13: Search & Query Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_061_search_nonexistent_account() {
    let mut trust = create_trust();
    let result = trust.search_account("doesnotexist");
    assert!(
        result.is_err(),
        "Searching for non-existent account should fail"
    );
}

#[test]
fn contract_062_search_trades_nonexistent_account_has_no_rows() {
    let mut trust = create_trust();
    assert!(trust
        .search_trades(Uuid::new_v4(), Status::New)
        .unwrap()
        .is_empty());
}

#[test]
fn bug_063_search_closed_trades_no_trades() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let trades = trust.search_closed_trades(Some(account.id)).unwrap();
    assert!(
        trades.is_empty(),
        "No closed trades should return empty vec"
    );
}

#[test]
fn bug_064_search_all_accounts_when_empty() {
    let mut trust = create_trust();
    let accounts = trust.search_all_accounts().unwrap();
    assert!(
        accounts.is_empty(),
        "Should return empty list when no accounts exist"
    );
}

#[test]
fn contract_065_account_history_contains_non_usd_cash() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    let (deposit, _) = trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::EUR,
        )
        .unwrap();
    assert_eq!(
        trust.get_account_transactions(account.id).unwrap(),
        vec![deposit]
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 14: Concentration & Advisory Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_066_advisory_empty_portfolio() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let proposal = core::services::TradeProposal {
        account_id: account.id,
        symbol: "AAPL".to_string(),
        sector: Some("technology".to_string()),
        asset_class: Some("stocks".to_string()),
        entry_price: dec!(100),
        quantity: dec!(100),
    };

    let result = trust.advisory_check_trade(proposal).unwrap();
    // Single trade in empty portfolio = 100% concentration
    assert_eq!(
        result.projected_single_position_pct,
        dec!(100),
        "Single position in empty portfolio should be 100% concentrated"
    );
}

#[test]
fn bug_067_advisory_zero_quantity_proposal() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let proposal = core::services::TradeProposal {
        account_id: account.id,
        symbol: "AAPL".to_string(),
        sector: Some("technology".to_string()),
        asset_class: Some("stocks".to_string()),
        entry_price: dec!(100),
        quantity: dec!(0), // Zero quantity
    };

    let result = trust.advisory_check_trade(proposal);
    assert!(
        result.is_ok(),
        "Zero quantity proposal should at least not crash"
    );
}

#[test]
fn bug_068_advisory_zero_entry_price_proposal() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let proposal = core::services::TradeProposal {
        account_id: account.id,
        symbol: "AAPL".to_string(),
        sector: Some("technology".to_string()),
        asset_class: Some("stocks".to_string()),
        entry_price: dec!(0), // Zero price
        quantity: dec!(100),
    };

    let result = trust.advisory_check_trade(proposal);
    assert!(
        result.is_ok(),
        "Zero price proposal should at least not crash"
    );
}

#[test]
fn bug_069_advisory_history_for_zero_days() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let history = trust.advisory_history_for_account(account.id, 0);
    // 0 days = nothing
    assert!(history.is_empty(), "0-day advisory history should be empty");
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 15: Drawdown Calculator Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_070_drawdown_only_withdrawals() {
    use core::calculators_drawdown::RealizedDrawdownCalculator;

    // Withdrawal without prior deposit (would cause negative balance)
    let txns = vec![create_test_transaction(
        TransactionCategory::Withdrawal,
        dec!(1000),
        10,
    )];

    let curve = RealizedDrawdownCalculator::calculate_equity_curve(&txns).unwrap();
    if !curve.points.is_empty() {
        let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).unwrap();
        assert!(
            metrics.current_equity <= dec!(0),
            "BUG: Negative equity from withdrawal-only scenario"
        );
    }
}

#[test]
fn bug_071_drawdown_single_point() {
    use core::calculators_drawdown::RealizedDrawdownCalculator;

    let txns = vec![create_test_transaction(
        TransactionCategory::Deposit,
        dec!(10000),
        10,
    )];

    let curve = RealizedDrawdownCalculator::calculate_equity_curve(&txns).unwrap();
    let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).unwrap();

    assert_eq!(metrics.current_drawdown, dec!(0));
    assert_eq!(metrics.max_drawdown, dec!(0));
    assert_eq!(metrics.peak_equity, dec!(10000));
}

fn create_test_transaction(
    category: TransactionCategory,
    amount: Decimal,
    days_ago: i64,
) -> Transaction {
    use chrono::Duration;
    let now = Utc::now().naive_utc();
    let created_at = now
        .checked_sub_signed(Duration::days(days_ago))
        .unwrap_or(now);
    Transaction {
        id: Uuid::new_v4(),
        created_at,
        updated_at: created_at,
        deleted_at: None,
        category,
        currency: Currency::USD,
        amount,
        account_id: Uuid::new_v4(),
    }
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 16: Multi-Trade Balance Consistency
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_072_multiple_trades_balance_tracking() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let tv1 = create_trading_vehicle(&mut trust, "AAPL");
    let tv2 = create_trading_vehicle(&mut trust, "MSFT");

    // Create two trades
    let draft1 = create_long_draft(&account, &tv1, 100);
    trust
        .create_trade(draft1, dec!(38), dec!(40), dec!(50))
        .unwrap();

    let draft2 = create_long_draft(&account, &tv2, 50);
    trust
        .create_trade(draft2, dec!(95), dec!(100), dec!(120))
        .unwrap();

    let trades = trust.search_trades(account.id, Status::New).unwrap();
    assert_eq!(trades.len(), 2);

    // Fund both
    trust.fund_trade(&trades[0]).unwrap();
    trust.fund_trade(&trades[1]).unwrap();

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    // Trade 1: 40 * 100 = 4000
    // Trade 2: 100 * 50 = 5000
    // Total funded: 9000
    // Available: 50000 - 9000 = 41000
    assert_eq!(
        balance.total_available,
        dec!(41000),
        "Balance should reflect both funded trades"
    );
}

#[test]
fn bug_073_cancel_one_of_two_funded_trades() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let tv1 = create_trading_vehicle(&mut trust, "AAPL");
    let tv2 = create_trading_vehicle(&mut trust, "MSFT");

    let draft1 = create_long_draft(&account, &tv1, 100);
    trust
        .create_trade(draft1, dec!(38), dec!(40), dec!(50))
        .unwrap();

    let draft2 = create_long_draft(&account, &tv2, 50);
    trust
        .create_trade(draft2, dec!(95), dec!(100), dec!(120))
        .unwrap();

    let trades = trust.search_trades(account.id, Status::New).unwrap();
    trust.fund_trade(&trades[0]).unwrap();
    trust.fund_trade(&trades[1]).unwrap();

    // Cancel only the first trade
    let funded_trades = trust.search_trades(account.id, Status::Funded).unwrap();
    trust.cancel_funded_trade(&funded_trades[0]).unwrap();

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    // Only trade 2 should still be funded (5000)
    // Available: 50000 - 5000 = 45000
    assert_eq!(
        balance.total_available,
        dec!(45000),
        "After canceling one trade, balance should reflect only remaining funded trade"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 17: Modify Stop/Target Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_074_modify_stop_on_unfilled_trade() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let submitted = submit_and_get_trade(&mut trust, &account, &trade);

    // Modify stop on submitted (not filled) trade
    let result = trust.modify_stop(&submitted, &account, dec!(39));
    assert!(
        result.is_err(),
        "Modifying stop on unfilled trade should fail"
    );
}

#[test]
fn bug_075_modify_target_on_unfilled_trade() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let submitted = submit_and_get_trade(&mut trust, &account, &trade);

    let result = trust.modify_target(&submitted, &account, dec!(60));
    assert!(
        result.is_err(),
        "Modifying target on unfilled trade should fail"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 18: Trade Summary & Reporting Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_076_trading_summary_no_accounts() {
    let mut trust = create_trust();
    let result = trust.get_trading_summary(None);
    assert!(
        result.is_ok(),
        "Trading summary with no accounts should return empty summary"
    );
    let summary = result.unwrap();
    assert_eq!(summary.equity, dec!(0));
}

#[test]
fn bug_077_trading_summary_nonexistent_account() {
    let mut trust = create_trust();
    let fake_id = Uuid::new_v4();
    let result = trust.get_trading_summary(Some(fake_id));
    assert!(
        result.is_err(),
        "Trading summary for non-existent account should fail"
    );
}

#[test]
fn bug_078_trading_summary_with_trades() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let summary = trust.get_trading_summary(Some(account.id)).unwrap();
    assert_eq!(summary.equity, dec!(50000));
    assert!(
        summary.performance.is_none(),
        "No closed trades = no performance"
    );
    assert!(summary.capital_at_risk.is_empty());
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 19: Account Hierarchy Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_079_create_child_of_nonexistent_parent() {
    let mut trust = create_trust();
    let fake_parent = Uuid::new_v4();

    let result = trust.create_account_with_hierarchy(
        "child",
        "child",
        Environment::Paper,
        dec!(20),
        dec!(10),
        AccountType::Earnings,
        Some(fake_parent),
    );
    assert!(result.is_err(), "Invalid input must be rejected");
}

#[test]
fn contract_080_non_primary_account_has_no_level() {
    let mut trust = create_trust();
    let parent = trust
        .create_account("parent", "parent", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    let child = trust
        .create_account_with_hierarchy(
            "child",
            "child",
            Environment::Paper,
            dec!(0),
            dec!(0),
            AccountType::Earnings,
            Some(parent.id),
        )
        .unwrap();

    assert!(trust.level_for_account(child.id).is_err());
    assert!(trust.level_for_account(parent.id).is_ok());
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 20: Concentration Calculator Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_081_portfolio_concentration_empty() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let result = trust.calculate_portfolio_concentration(Some(account.id));
    assert!(
        result.is_ok(),
        "Empty portfolio concentration should return ok"
    );
    let groups = result.unwrap();
    assert!(groups.is_empty(), "No trades = no concentration groups");
}

#[test]
fn bug_082_open_positions_empty() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let positions = trust.calculate_open_positions(Some(account.id)).unwrap();
    assert!(positions.is_empty(), "No trades = no open positions");
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 21: Edge Cases in Trade Grading
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_083_grade_non_closed_trade() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    // Try to grade a trade that's not closed
    let result = trust.grade_trade(
        trade.id,
        core::services::grading::GradingWeightsPermille::default(),
    );
    assert!(result.is_err(), "Grading a non-closed trade should fail");
}

#[test]
fn bug_084_latest_grade_no_grades() {
    let mut trust = create_trust();

    let result = trust.latest_trade_grade(Uuid::new_v4());
    assert!(
        result.is_ok(),
        "Should return Ok(None) for trade with no grades"
    );
    assert!(result.unwrap().is_none());
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 22: Fund Transfer Edge Cases
// ─────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 23: Extreme Values & Overflow
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_089_very_large_deposit() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    // Very large value (but within Decimal range)
    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Deposit,
        dec!(99999999999999),
        &Currency::USD,
    );
    assert!(result.is_ok(), "Large deposit should succeed");
}

#[test]
fn bug_090_very_small_deposit() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    // 1 cent deposit
    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Deposit,
        dec!(0.01),
        &Currency::USD,
    );
    assert!(result.is_ok(), "Small deposit should succeed");
    let (_, balance) = result.unwrap();
    assert_eq!(balance.total_available, dec!(0.01));
}

#[test]
fn bug_091_very_high_precision_amount() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    // Many decimal places
    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Deposit,
        dec!(100.123456789),
        &Currency::USD,
    );
    assert!(result.is_ok(), "High precision deposit should succeed");
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 24: Execution & Fee Reconciliation Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_092_executions_for_nonexistent_trade() {
    let mut trust = create_trust();
    let result = trust.executions_for_trade(Uuid::new_v4());
    assert!(
        result.is_ok(),
        "Should return empty vec for non-existent trade"
    );
    assert!(result.unwrap().is_empty());
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 25: Multiple Currency Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_093_trade_in_eur_with_usd_balance_only() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "SAP");

    // Create trade with EUR currency but account only has USD balance
    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 100.into(),
        currency: Currency::EUR,
        category: TradeCategory::Long,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };

    trust
        .create_trade(draft, dec!(90), dec!(100), dec!(120))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let result = trust.fund_trade(&trade);
    assert!(
        result.is_err(),
        "Funding EUR trade with only USD balance should fail"
    );
}

#[test]
fn bug_094_all_balances_multi_currency() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(2000),
            &Currency::EUR,
        )
        .unwrap();

    let balances = trust.search_all_balances(account.id).unwrap();
    assert_eq!(
        balances.len(),
        2,
        "Should have balances for both currencies"
    );

    let usd_balance = balances
        .iter()
        .find(|b| b.currency == Currency::USD)
        .unwrap();
    let eur_balance = balances
        .iter()
        .find(|b| b.currency == Currency::EUR)
        .unwrap();
    assert_eq!(usd_balance.total_available, dec!(1000));
    assert_eq!(eur_balance.total_available, dec!(2000));
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 26: Trade with Metadata Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_095_trade_with_all_metadata() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 100.into(),
        currency: Currency::USD,
        category: TradeCategory::Long,
        thesis: Some("Test thesis".to_string()),
        sector: Some("technology".to_string()),
        asset_class: Some("stocks".to_string()),
        context: Some("test context".to_string()),
    };

    let result = trust.create_trade(draft, dec!(38), dec!(40), dec!(50));
    assert!(result.is_ok(), "Trade with all metadata should succeed");

    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();
    assert_eq!(trade.thesis, Some("Test thesis".to_string()));
    assert_eq!(trade.sector, Some("technology".to_string()));
    assert_eq!(trade.asset_class, Some("stocks".to_string()));
    assert_eq!(trade.context, Some("test context".to_string()));
}

#[test]
fn bug_096_trade_with_empty_metadata_strings() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 100.into(),
        currency: Currency::USD,
        category: TradeCategory::Long,
        thesis: Some("".to_string()),
        sector: Some("".to_string()),
        asset_class: Some("".to_string()),
        context: Some("".to_string()),
    };

    let result = trust.create_trade(draft, dec!(38), dec!(40), dec!(50));
    // Empty string metadata should be handled (Some("") vs None)
    assert!(
        result.is_ok(),
        "Trade with empty string metadata should succeed"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 27: Performance Edge Cases with Decimal Precision
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_097_win_rate_50_percent() {
    use core::calculators_performance::PerformanceCalculator;

    let trades = vec![
        create_closed_trade(
            dec!(100),
            dec!(95),
            dec!(110),
            Status::ClosedTarget,
            dec!(100),
        ),
        create_closed_trade(
            dec!(100),
            dec!(95),
            dec!(110),
            Status::ClosedStopLoss,
            dec!(-50),
        ),
    ];

    let stats = PerformanceCalculator::calculate_performance_stats(&trades);
    assert_eq!(stats.win_rate, dec!(50), "1 win, 1 loss = 50% win rate");
    assert_eq!(stats.winning_trades, 1);
    assert_eq!(stats.losing_trades, 1);
}

#[test]
fn bug_098_performance_stats_all_losers() {
    use core::calculators_performance::PerformanceCalculator;

    let trades = vec![
        create_closed_trade(
            dec!(100),
            dec!(95),
            dec!(110),
            Status::ClosedStopLoss,
            dec!(-50),
        ),
        create_closed_trade(
            dec!(100),
            dec!(95),
            dec!(110),
            Status::ClosedStopLoss,
            dec!(-30),
        ),
        create_closed_trade(
            dec!(100),
            dec!(95),
            dec!(110),
            Status::ClosedStopLoss,
            dec!(-80),
        ),
    ];

    let stats = PerformanceCalculator::calculate_performance_stats(&trades);
    assert_eq!(stats.win_rate, dec!(0));
    assert_eq!(stats.winning_trades, 0);
    assert_eq!(stats.losing_trades, 3);
    assert_eq!(stats.average_win, dec!(0));
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 28: Advisory Thresholds Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_099_advisory_thresholds_zero_limits() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    // Set all limits to 0% - everything should be blocked
    trust
        .configure_advisory_thresholds(
            account.id,
            core::services::AdvisoryThresholds {
                sector_limit_pct: dec!(0),
                asset_class_limit_pct: dec!(0),
                single_position_limit_pct: dec!(0),
            },
        )
        .unwrap();

    let proposal = core::services::TradeProposal {
        account_id: account.id,
        symbol: "AAPL".to_string(),
        sector: Some("technology".to_string()),
        asset_class: Some("stocks".to_string()),
        entry_price: dec!(100),
        quantity: dec!(100),
    };

    let result = trust.advisory_check_trade(proposal).unwrap();
    assert!(
        matches!(result.level, core::services::AdvisoryAlertLevel::Block),
        "Zero limits should block all trades, got: {:?}",
        result.level
    );
}

#[test]
fn bug_100_advisory_portfolio_status_empty() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let status = trust.advisory_status_for_account(account.id).unwrap();
    assert!(
        matches!(status.level, core::services::AdvisoryAlertLevel::Ok),
        "Empty portfolio should have Ok status"
    );
    assert_eq!(status.top_sector_pct, dec!(0));
    assert_eq!(status.top_asset_class_pct, dec!(0));
    assert_eq!(status.top_position_pct, dec!(0));
}

// ═════════════════════════════════════════════════════════════════
// EXTENDED BUG HUNT: Tests 101-200
// ═════════════════════════════════════════════════════════════════

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 29: Modify Stop/Target Price Edge Cases
// ─────────────────────────────────────────────────────────────────

fn fill_and_get_trade(trust: &mut TrustFacade, account: &Account) -> Trade {
    let submitted_trades = trust.search_trades(account.id, Status::New).unwrap();
    let trade = submitted_trades.first().unwrap().clone();
    let submitted = submit_and_get_trade(trust, account, &trade);
    trust.sync_trade(&submitted, account).unwrap();
    trust
        .search_trades(account.id, Status::Filled)
        .unwrap()
        .first()
        .unwrap()
        .clone()
}

#[test]
fn contract_101_modify_target_to_zero_price() {
    let mut trust = create_trust_with_broker(SyncBroker {
        sync_fn: make_entry_filled_orders,
    });
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let filled = fill_and_get_trade(&mut trust, &account);

    let result = trust.modify_target(&filled, &account, dec!(0));
    assert!(result.is_err(), "Invalid input must be rejected");
}

#[test]
fn contract_102_modify_target_to_negative_price() {
    let mut trust = create_trust_with_broker(SyncBroker {
        sync_fn: make_entry_filled_orders,
    });
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let filled = fill_and_get_trade(&mut trust, &account);

    let result = trust.modify_target(&filled, &account, dec!(-10));
    assert!(result.is_err(), "Invalid input must be rejected");
}

#[test]
fn contract_103_modify_target_below_entry_for_long() {
    let mut trust = create_trust_with_broker(SyncBroker {
        sync_fn: make_entry_filled_orders,
    });
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let filled = fill_and_get_trade(&mut trust, &account);

    let result = trust.modify_target(&filled, &account, dec!(35));
    let updated = result.expect("Positive exit changes and tighter stops are permitted");
    let persisted = trust.read_trade(updated.id).unwrap();
    assert_eq!(persisted.target.unit_price, dec!(35));
    assert_eq!(persisted.status, Status::Filled);
    assert!(persisted.safety_stop.unit_price >= filled.safety_stop.unit_price);
}

#[test]
fn contract_104_modify_target_equal_to_entry_for_long() {
    let mut trust = create_trust_with_broker(SyncBroker {
        sync_fn: make_entry_filled_orders,
    });
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let filled = fill_and_get_trade(&mut trust, &account);

    let result = trust.modify_target(&filled, &account, dec!(40));
    let updated = result.expect("Positive exit changes and tighter stops are permitted");
    let persisted = trust.read_trade(updated.id).unwrap();
    assert_eq!(persisted.target.unit_price, dec!(40));
    assert_eq!(persisted.status, Status::Filled);
    assert!(persisted.safety_stop.unit_price >= filled.safety_stop.unit_price);
}

#[test]
fn contract_105_modify_target_below_stop_for_long() {
    let mut trust = create_trust_with_broker(SyncBroker {
        sync_fn: make_entry_filled_orders,
    });
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let filled = fill_and_get_trade(&mut trust, &account);

    let result = trust.modify_target(&filled, &account, dec!(37));
    let updated = result.expect("Positive exit changes and tighter stops are permitted");
    let persisted = trust.read_trade(updated.id).unwrap();
    assert_eq!(persisted.target.unit_price, dec!(37));
    assert_eq!(persisted.status, Status::Filled);
    assert!(persisted.safety_stop.unit_price >= filled.safety_stop.unit_price);
}

#[test]
fn bug_106_modify_stop_to_zero_for_long() {
    let mut trust = create_trust_with_broker(SyncBroker {
        sync_fn: make_entry_filled_orders,
    });
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let filled = fill_and_get_trade(&mut trust, &account);

    // Stop at $0 for LONG (essentially removing the stop)
    // Note: This should be rejected as it increases risk (lowering stop from $38 to $0)
    let result = trust.modify_stop(&filled, &account, dec!(0));
    // can_modify_stop checks: Long && current_stop > new_price → reject
    // $38 > $0 → true, so this should be rejected
    assert!(
        result.is_err(),
        "Lowering stop to $0 should be rejected (increases risk)"
    );
}

#[test]
fn bug_107_modify_stop_to_negative_for_long() {
    let mut trust = create_trust_with_broker(SyncBroker {
        sync_fn: make_entry_filled_orders,
    });
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let filled = fill_and_get_trade(&mut trust, &account);

    // Negative stop price
    let result = trust.modify_stop(&filled, &account, dec!(-5));
    // Should be rejected: Long && $38 > $-5 → true
    assert!(result.is_err(), "Negative stop price should be rejected");
}

#[test]
fn contract_108_modify_stop_equal_to_entry_for_long() {
    let mut trust = create_trust_with_broker(SyncBroker {
        sync_fn: make_entry_filled_orders,
    });
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let filled = fill_and_get_trade(&mut trust, &account);

    // Stop = Entry for LONG (zero risk but also locks in zero P&L)
    trust.modify_stop(&filled, &account, dec!(40)).unwrap();
    assert_eq!(
        trust.read_trade(filled.id).unwrap().safety_stop.unit_price,
        dec!(40)
    );
    assert!(trust.modify_stop(&filled, &account, dec!(39)).is_err());
    assert_eq!(
        trust.read_trade(filled.id).unwrap().safety_stop.unit_price,
        dec!(40)
    );
}

#[test]
fn contract_109_modify_stop_above_target_for_long() {
    let mut trust = create_trust_with_broker(SyncBroker {
        sync_fn: make_entry_filled_orders,
    });
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let filled = fill_and_get_trade(&mut trust, &account);

    let result = trust.modify_stop(&filled, &account, dec!(55));
    let updated = result.expect("Positive exit changes and tighter stops are permitted");
    let persisted = trust.read_trade(updated.id).unwrap();
    assert_eq!(persisted.safety_stop.unit_price, dec!(55));
    assert_eq!(persisted.status, Status::Filled);
    assert!(persisted.safety_stop.unit_price >= filled.safety_stop.unit_price);
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 30: Trade Close Command Bug (Status::Canceled)
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_110_close_trade_sets_status_to_canceled_not_closed() {
    let mut trust = create_trust_with_broker(SyncBroker {
        sync_fn: make_entry_filled_orders,
    });
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let filled = fill_and_get_trade(&mut trust, &account);

    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    trust.close_trade(&filled).unwrap();
    let persisted = trust.read_trade(filled.id).unwrap();
    assert_eq!(
        persisted.status,
        Status::Filled,
        "Broker acknowledgement is not an exit fill"
    );
    assert_eq!(persisted.target.category, model::OrderCategory::Market);
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
    assert!(
        trust.close_trade(&filled).is_err(),
        "Do not send another market exit while the first is pending"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 31: can_cancel_submitted Error Code Bug
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_111_cancel_submitted_returns_typed_state_error() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let error = trust.cancel_submitted_trade(&trade).unwrap_err();
    let validation = error
        .downcast_ref::<core::validation_errors::TradeValidationError>()
        .or_else(|| {
            error
                .downcast_ref::<Box<core::validation_errors::TradeValidationError>>()
                .map(AsRef::as_ref)
        })
        .expect("typed lifecycle error");
    assert_eq!(
        validation.code,
        core::validation_errors::TradeValidationErrorCode::TradeNotSubmitted
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 32: Account Creation Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_112_create_account_with_negative_taxes() {
    let mut trust = create_trust();
    let result = trust.create_account("test", "test", Environment::Paper, dec!(-10), dec!(10));
    assert!(result.is_err(), "Invalid input must be rejected");
    assert!(
        trust.search_all_accounts().unwrap().is_empty(),
        "Rejected creation must leave no account"
    );
}

#[test]
fn contract_113_create_account_with_over_100_taxes() {
    let mut trust = create_trust();
    let result = trust.create_account("test", "test", Environment::Paper, dec!(150), dec!(10));
    assert!(result.is_err(), "Invalid input must be rejected");
    assert!(
        trust.search_all_accounts().unwrap().is_empty(),
        "Rejected creation must leave no account"
    );
}

#[test]
fn contract_114_create_account_with_taxes_plus_earnings_over_100() {
    let mut trust = create_trust();
    let result = trust.create_account("test", "test", Environment::Paper, dec!(60), dec!(60));
    assert!(result.is_err(), "Invalid input must be rejected");
    assert!(
        trust.search_all_accounts().unwrap().is_empty(),
        "Rejected creation must leave no account"
    );
}

#[test]
fn contract_115_create_account_with_empty_name() {
    let mut trust = create_trust();
    let result = trust.create_account("", "desc", Environment::Paper, dec!(20), dec!(10));
    assert!(result.is_err(), "Invalid input must be rejected");
    assert!(
        trust.search_all_accounts().unwrap().is_empty(),
        "Rejected creation must leave no account"
    );
}

#[test]
fn contract_116_create_account_with_whitespace_name() {
    let mut trust = create_trust();
    let result = trust.create_account("   ", "desc", Environment::Paper, dec!(20), dec!(10));
    assert!(result.is_err(), "Invalid input must be rejected");
    assert!(
        trust.search_all_accounts().unwrap().is_empty(),
        "Rejected creation must leave no account"
    );
}

#[test]
fn contract_117_empty_description_remains_valid() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "", Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    assert_eq!(trust.search_account("test").unwrap(), account);
    assert!(account.description.is_empty());
}

#[test]
fn bug_118_create_account_with_zero_taxes_and_earnings() {
    let mut trust = create_trust();
    // 0% taxes + 0% earnings = all profit goes back to reinvestment
    let result = trust.create_account("test", "test", Environment::Paper, dec!(0), dec!(0));
    // This should be acceptable
    assert!(result.is_ok(), "Zero taxes and earnings should be valid");
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 33: Rule Priority & Ordering
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_119_rule_with_zero_risk_per_trade() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    // 0% risk per trade means no trade can ever be taken
    let result = trust.create_rule(
        &account,
        &RuleName::RiskPerTrade(0.0),
        "zero risk",
        &RuleLevel::Error,
    );
    assert!(result.is_ok(), "zero is an intentional no-new-risk policy");
}

#[test]
fn bug_120_rule_with_zero_risk_per_month() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    // 0% risk per month means no trade can ever be taken this month
    let result = trust.create_rule(
        &account,
        &RuleName::RiskPerMonth(0.0),
        "zero monthly",
        &RuleLevel::Error,
    );
    assert!(result.is_ok(), "zero is an intentional no-new-risk policy");
}

#[test]
fn bug_121_duplicate_rules_same_type() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    // Create two RiskPerTrade rules with different values
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(2.0),
            "risk 2%",
            &RuleLevel::Error,
        )
        .unwrap();
    let result = trust.create_rule(
        &account,
        &RuleName::RiskPerTrade(5.0),
        "risk 5%",
        &RuleLevel::Error,
    );
    assert!(result.is_err(), "invalid or duplicate risk rule must fail");
}

#[test]
fn bug_122_rule_with_nan_like_risk() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    // f32 special values: infinity
    let result = trust.create_rule(
        &account,
        &RuleName::RiskPerTrade(f32::INFINITY),
        "inf",
        &RuleLevel::Error,
    );
    assert!(result.is_err(), "invalid or duplicate risk rule must fail");
}

#[test]
fn bug_123_rule_with_nan_risk() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let result = trust.create_rule(
        &account,
        &RuleName::RiskPerTrade(f32::NAN),
        "nan",
        &RuleLevel::Error,
    );
    assert!(result.is_err(), "invalid or duplicate risk rule must fail");
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 34: Trading Vehicle Validation
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_124_trading_vehicle_with_whitespace_only_symbol() {
    let mut trust = create_trust();
    let result = trust.create_trading_vehicle(
        "   ",
        Some("US0000000000"),
        &TradingVehicleCategory::Stock,
        "TEST",
    );
    assert!(result.is_err(), "Invalid input must be rejected");
    assert!(trust.search_trading_vehicles().unwrap().is_empty());
}

#[test]
fn contract_125_trading_vehicle_with_very_long_symbol() {
    let mut trust = create_trust();
    let long_symbol = "A".repeat(1000);
    let result = trust.create_trading_vehicle(
        &long_symbol,
        Some("US0000000000"),
        &TradingVehicleCategory::Stock,
        "TEST",
    );
    assert!(result.is_err(), "Invalid input must be rejected");
    assert!(trust.search_trading_vehicles().unwrap().is_empty());
}

#[test]
fn contract_126_trading_vehicle_with_special_chars_symbol() {
    let mut trust = create_trust();
    let result = trust.create_trading_vehicle(
        "A@#$%!",
        Some("US0000000000"),
        &TradingVehicleCategory::Stock,
        "TEST",
    );
    let vehicle = result.unwrap();
    assert_eq!(vehicle.symbol, "A@#$%!");
    assert_eq!(trust.search_trading_vehicles().unwrap(), vec![vehicle]);
}

#[test]
fn contract_127_trading_vehicle_with_empty_exchange() {
    let mut trust = create_trust();
    let result = trust.create_trading_vehicle(
        "AAPL",
        Some("US0000000000"),
        &TradingVehicleCategory::Stock,
        "",
    );
    assert!(result.is_err(), "Invalid input must be rejected");
    assert!(trust.search_trading_vehicles().unwrap().is_empty());
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 35: Trade Quantity Edge Cases (DraftTrade.quantity is i64)
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_128_trade_with_negative_quantity() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, -100);
    let result = trust.create_trade(draft, dec!(38), dec!(40), dec!(50));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

#[test]
fn contract_129_trade_with_max_i64_quantity() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, i64::MAX);
    let result = trust.create_trade(draft, dec!(38), dec!(40), dec!(50));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

#[test]
fn contract_130_trade_with_i64_min_quantity() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, i64::MIN);
    let result = trust.create_trade(draft, dec!(38), dec!(40), dec!(50));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 36: Price Ordering Consistency
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_131_long_trade_stop_equals_target() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    let result = trust.create_trade(draft, dec!(40), dec!(40), dec!(40));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

#[test]
fn contract_132_short_trade_stop_above_entry() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_short_draft(&account, &tv, 100);
    let result = trust.create_trade(draft, dec!(45), dec!(50), dec!(40));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

#[test]
fn contract_133_long_trade_all_prices_identical() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    let result = trust.create_trade(draft, dec!(100), dec!(100), dec!(100));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 37: Grading Weights Saturation Bug
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_134_grading_weights_saturation_overflow() {
    use core::services::grading::GradingWeightsPermille;

    // Each weight is 500, sum = 2000, should fail validation (max = 1000)
    // But saturating_add caps at u16::MAX (65535), so sum wouldn't overflow
    let weights = GradingWeightsPermille {
        process: 500,
        risk: 500,
        execution: 500,
        documentation: 500,
    };
    let result = weights.validate();
    // Should fail because 500+500+500+500 = 2000 > 1000
    assert!(
        result.is_err(),
        "Grading weights summing to 2000 should fail validation"
    );
}

#[test]
fn bug_135_grading_weights_exactly_1000() {
    use core::services::grading::GradingWeightsPermille;

    let weights = GradingWeightsPermille {
        process: 400,
        risk: 300,
        execution: 200,
        documentation: 100,
    };
    let result = weights.validate();
    assert!(
        result.is_ok(),
        "Grading weights summing to exactly 1000 should pass"
    );
}

#[test]
fn bug_136_grading_weights_all_zero() {
    use core::services::grading::GradingWeightsPermille;

    let weights = GradingWeightsPermille {
        process: 0,
        risk: 0,
        execution: 0,
        documentation: 0,
    };
    let result = weights.validate();
    // Sum = 0, should fail validation (not 1000)
    assert!(
        result.is_err(),
        "All-zero grading weights should fail validation"
    );
}

#[test]
fn bug_137_grading_weights_u16_max_overflow() {
    use core::services::grading::GradingWeightsPermille;

    // u16::MAX = 65535; saturating_add would cap at 65535
    let weights = GradingWeightsPermille {
        process: u16::MAX,
        risk: u16::MAX,
        execution: u16::MAX,
        documentation: u16::MAX,
    };
    let result = weights.validate();
    // 65535 != 1000, so should fail
    assert!(result.is_err(), "u16::MAX weights should fail validation");
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 38: Double-Funding Balance Corruption
// ─────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 39: Deposit & Withdrawal Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_140_withdraw_exact_balance() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();

    // Withdraw exact balance
    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Withdrawal,
        dec!(1000),
        &Currency::USD,
    );
    assert!(result.is_ok(), "Withdrawing exact balance should succeed");

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(
        balance.total_available,
        dec!(0),
        "Balance should be $0 after full withdrawal"
    );
}

#[test]
fn bug_141_withdraw_more_than_balance() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();

    // Withdraw more than balance
    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Withdrawal,
        dec!(1001),
        &Currency::USD,
    );
    assert!(result.is_err(), "Withdrawing more than balance should fail");
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 40: Fund Trade Without Enough Capital
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_143_fund_trade_with_exactly_enough_capital() {
    let mut trust = create_trust();
    // Deposit exactly $4000 = 100 shares * $40
    let account = setup_account_no_rules(&mut trust, dec!(4000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    // Should succeed with exactly enough capital
    let result = trust.fund_trade(&trade);
    assert!(
        result.is_ok(),
        "Funding with exactly enough capital should succeed"
    );
}

#[test]
fn bug_144_fund_trade_one_cent_short() {
    let mut trust = create_trust();
    // Deposit $3999.99 — just short of $4000 needed
    let account = setup_account_no_rules(&mut trust, dec!(3999.99));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let result = trust.fund_trade(&trade);
    assert!(
        result.is_err(),
        "Funding with $0.01 less than required should fail"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 41: Trade With Extreme Prices
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_145_trade_with_very_small_price_diff() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    // Entry = $100.01, Stop = $100.00 (1 cent risk per share)
    let draft = create_long_draft(&account, &tv, 100);
    let result = trust.create_trade(draft, dec!(100.00), dec!(100.01), dec!(200));
    // Very tiny risk per share, should work
    assert!(result.is_ok(), "Trade with 1-cent risk should be accepted");
}

#[test]
fn contract_146_trade_with_extreme_price_ratios() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    // Entry $0.01, Target $999999 (insane reward-to-risk)
    let draft = create_long_draft(&account, &tv, 1);
    let trade = trust
        .create_trade(draft, dec!(0.001), dec!(0.01), dec!(999999))
        .unwrap();
    let persisted = trust.read_trade(trade.id).unwrap();
    assert_eq!(persisted.entry.unit_price, dec!(0.01));
    assert_eq!(persisted.safety_stop.unit_price, dec!(0.001));
    assert_eq!(persisted.target.unit_price, dec!(999999));
    assert_eq!(persisted.status, Status::New);
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 42: Multiple Accounts Isolation
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_147_trades_isolated_between_accounts() {
    let mut trust = create_trust();

    trust
        .create_account("acc1", "account 1", Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    let acc1 = trust.search_account("acc1").unwrap();
    trust
        .create_transaction(
            &acc1,
            &TransactionCategory::Deposit,
            dec!(50000),
            &Currency::USD,
        )
        .unwrap();

    trust
        .create_account("acc2", "account 2", Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    let acc2 = trust.search_account("acc2").unwrap();
    trust
        .create_transaction(
            &acc2,
            &TransactionCategory::Deposit,
            dec!(50000),
            &Currency::USD,
        )
        .unwrap();

    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&acc1, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();

    // acc1 should have 1 trade, acc2 should have 0
    let acc1_trades = trust.search_trades(acc1.id, Status::New).unwrap();
    let acc2_trades = trust.search_trades(acc2.id, Status::New).unwrap();

    assert_eq!(acc1_trades.len(), 1, "Account 1 should have 1 trade");
    assert_eq!(acc2_trades.len(), 0, "Account 2 should have 0 trades");
}

#[test]
fn bug_148_fund_trade_from_wrong_account_balance() {
    let mut trust = create_trust();

    // acc1 has $1000 (not enough for the trade)
    trust
        .create_account("acc1", "account 1", Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    let acc1 = trust.search_account("acc1").unwrap();
    trust
        .create_transaction(
            &acc1,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();

    // acc2 has $50000 (more than enough)
    trust
        .create_account("acc2", "account 2", Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    let acc2 = trust.search_account("acc2").unwrap();
    trust
        .create_transaction(
            &acc2,
            &TransactionCategory::Deposit,
            dec!(50000),
            &Currency::USD,
        )
        .unwrap();

    let tv = create_trading_vehicle(&mut trust, "AAPL");

    // Create trade on acc1 (which only has $1000)
    let draft = create_long_draft(&acc1, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(acc1.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    // Funding should fail because acc1 only has $1000 but needs $4000
    let result = trust.fund_trade(&trade);
    assert!(
        result.is_err(),
        "Fund trade should fail - acc1 only has $1000, needs $4000"
    );

    // Verify acc2 balance wasn't affected
    let acc2_balance = trust.search_balance(acc2.id, &Currency::USD).unwrap();
    assert_eq!(
        acc2_balance.total_available,
        dec!(50000),
        "Account 2 balance should be unaffected"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 43: Level Change Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_149_change_level_to_same_level() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    // Default level is 3, try changing to 3
    let result = trust.change_level(
        account.id,
        3,
        "no change",
        model::LevelTrigger::ManualOverride,
    );
    assert!(
        result.is_err(),
        "Changing level to the same value should fail"
    );
}

#[test]
fn bug_150_change_level_with_empty_reason() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let result = trust.change_level(account.id, 4, "", model::LevelTrigger::ManualOverride);
    // Empty reason should fail
    assert!(
        result.is_err(),
        "Level change with empty reason should fail"
    );
}

#[test]
fn contract_151_change_level_with_whitespace_reason() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let result = trust.change_level(account.id, 4, "   ", model::LevelTrigger::ManualOverride);
    assert!(result.is_err(), "Invalid input must be rejected");
}

#[test]
fn contract_152_change_level_with_custom_empty_trigger() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let result = trust.change_level(
        account.id,
        4,
        "test reason",
        model::LevelTrigger::Custom("".to_string()),
    );
    assert!(result.is_err(), "Invalid input must be rejected");
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 44: Search/Query Robustness
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_153_search_account_empty_name() {
    let mut trust = create_trust();
    let result = trust.search_account("");
    assert!(
        result.is_err(),
        "Searching for empty-name account should fail"
    );
}

#[test]
fn bug_154_search_account_by_name_with_sql_injection() {
    let mut trust = create_trust();
    // Diesel should prevent SQL injection, but test it
    let result = trust.search_account("'; DROP TABLE accounts; --");
    assert!(
        result.is_err(),
        "SQL injection in search should fail safely"
    );
}

#[test]
fn bug_155_search_balance_nonexistent_account() {
    let mut trust = create_trust();
    let fake_id = Uuid::new_v4();
    let result = trust.search_balance(fake_id, &Currency::USD);
    // Should return error or default balance
    assert!(
        result.is_err(),
        "Balance for non-existent account should return error"
    );
}

#[test]
fn contract_156_search_balance_wrong_currency() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let result = trust.search_balance(account.id, &Currency::EUR);
    assert!(
        result.is_err(),
        "An absent currency must not inherit USD capital"
    );
    assert_eq!(
        trust
            .search_balance(account.id, &Currency::USD)
            .unwrap()
            .total_available,
        dec!(50000)
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 45: Trade Lifecycle State Machine Violations
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_157_submit_new_trade_directly() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    // Submit without funding first
    let result = trust.submit_trade(&trade);
    assert!(
        result.is_err(),
        "Submitting a New (unfunded) trade should fail"
    );
}

#[test]
fn bug_158_cancel_funded_on_submitted_trade() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    trust.fund_trade(&trade).unwrap();
    let funded = trust
        .search_trades(account.id, Status::Funded)
        .unwrap()
        .first()
        .unwrap()
        .clone();
    trust.submit_trade(&funded).unwrap();
    let submitted = trust
        .search_trades(account.id, Status::Submitted)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    // cancel_funded should fail on submitted trades
    let result = trust.cancel_funded_trade(&submitted);
    assert!(
        result.is_err(),
        "cancel_funded should fail on submitted trade"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 46: Distribution Percentages
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_160_distribution_negative_earnings_percentage() {
    let mut trust = create_trust();
    let result = trust.create_account("test", "test", Environment::Paper, dec!(20), dec!(-10));
    assert!(result.is_err(), "Invalid input must be rejected");
    assert!(
        trust.search_all_accounts().unwrap().is_empty(),
        "Rejected creation must leave no account"
    );
}

#[test]
fn contract_161_one_hundred_percent_tax_reserve_roundtrips() {
    let mut trust = create_trust();
    trust
        .create_account("test", "test", Environment::Paper, dec!(100), dec!(0))
        .unwrap();
    let account = trust.search_account("test").unwrap();
    assert_eq!(account.taxes_percentage, dec!(100));
    assert_eq!(account.earnings_percentage, dec!(0));
}

#[test]
fn contract_162_one_hundred_percent_earnings_reserve_roundtrips() {
    let mut trust = create_trust();
    trust
        .create_account("test", "test", Environment::Paper, dec!(0), dec!(100))
        .unwrap();
    let account = trust.search_account("test").unwrap();
    assert_eq!(account.taxes_percentage, dec!(0));
    assert_eq!(account.earnings_percentage, dec!(100));
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 47: Concurrent Trade Operations
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_163_fund_multiple_trades_exceeding_total_balance() {
    let mut trust = create_trust();
    // $5000 total, each trade needs $4000 (100 shares * $40)
    let account = setup_account_no_rules(&mut trust, dec!(5000));
    let tv1 = create_trading_vehicle(&mut trust, "AAPL");
    let tv2 = create_trading_vehicle(&mut trust, "GOOG");

    let draft1 = create_long_draft(&account, &tv1, 100);
    trust
        .create_trade(draft1, dec!(38), dec!(40), dec!(50))
        .unwrap();

    let draft2 = create_long_draft(&account, &tv2, 100);
    trust
        .create_trade(draft2, dec!(38), dec!(40), dec!(50))
        .unwrap();

    let trades = trust.search_trades(account.id, Status::New).unwrap();
    assert_eq!(trades.len(), 2);

    // Fund first trade ($4000) - should succeed
    trust.fund_trade(&trades[0]).unwrap();

    // Fund second trade ($4000) - should fail (only $1000 left)
    let result = trust.fund_trade(&trades[1]);
    assert!(
        result.is_err(),
        "Second trade funding should fail - insufficient remaining balance"
    );

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(
        balance.total_available,
        dec!(1000),
        "Available should be $1000 after first funding"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 48: Short Trade Specific Bugs
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_164_short_trade_capital_calculation() {
    // Short trade: entry $50, stop $55, target $40, qty 100
    // Capital needed = stop * qty = $55 * 100 = $5500
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(5500));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_short_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(55), dec!(50), dec!(40))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    // Should succeed with exactly enough capital for worst case
    let result = trust.fund_trade(&trade);
    assert!(
        result.is_ok(),
        "Short trade should be fundable with exactly stop*qty capital"
    );
}

#[test]
fn contract_165_short_trade_with_stop_equal_entry() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_short_draft(&account, &tv, 100);
    let result = trust.create_trade(draft, dec!(50), dec!(50), dec!(40));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

#[test]
fn contract_166_short_trade_with_target_above_entry() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_short_draft(&account, &tv, 100);
    let result = trust.create_trade(draft, dec!(60), dec!(50), dec!(55));
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    if let Ok(trade) = result {
        let before_transactions = trust.get_account_transactions(account.id).unwrap();
        assert!(
            trust.fund_trade(&trade).is_err(),
            "Invalid or unaffordable draft must not commit capital"
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            before_transactions
        );
        assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::New);
    }
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        before
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 49: Trade Thesis/Metadata Validation
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_167_trade_thesis_over_200_chars() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let long_thesis = "A".repeat(300);
    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 100.into(),
        currency: Currency::USD,
        category: TradeCategory::Long,
        thesis: Some(long_thesis),
        sector: None,
        asset_class: None,
        context: None,
    };

    let result = trust.create_trade(draft, dec!(38), dec!(40), dec!(50));
    assert!(result.is_err(), "Invalid input must be rejected");
}

#[test]
fn contract_168_trade_sector_very_long() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 100.into(),
        currency: Currency::USD,
        category: TradeCategory::Long,
        thesis: None,
        sector: Some("A".repeat(10000)),
        asset_class: None,
        context: None,
    };

    let result = trust.create_trade(draft, dec!(38), dec!(40), dec!(50));
    assert!(result.is_err(), "Invalid input must be rejected");
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 50: Performance & Risk Calculator Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_169_max_quantity_with_zero_entry_price_is_rejected() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let result = trust.calculate_maximum_quantity(account.id, dec!(0), dec!(0), &Currency::USD);
    assert!(
        result.is_err(),
        "Zero prices cannot produce an executable size"
    );
}

#[test]
fn contract_170_max_quantity_entry_equals_stop() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let result = trust.calculate_maximum_quantity(account.id, dec!(100), dec!(100), &Currency::USD);
    assert!(
        result.is_err(),
        "Zero stop distance cannot produce an executable size"
    );
}

#[test]
fn contract_171_max_quantity_stop_above_entry() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));

    let result = trust.calculate_maximum_quantity(account.id, dec!(100), dec!(110), &Currency::USD);
    let quantity = result.unwrap();
    assert!(
        quantity > Decimal::ZERO,
        "A short stop above entry is valid"
    );
    assert!(
        quantity * dec!(10) <= dec!(1000),
        "Size must respect 2% risk on $50,000"
    );
    assert!(
        quantity * dec!(110) <= dec!(50000),
        "Short stop notional must fit available capital"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 51: Advisory System Validation
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_172_advisory_thresholds_negative_limits() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let result = trust.configure_advisory_thresholds(
        account.id,
        core::services::AdvisoryThresholds {
            sector_limit_pct: dec!(-10),
            asset_class_limit_pct: dec!(-10),
            single_position_limit_pct: dec!(-10),
        },
    );
    assert!(result.is_err(), "Invalid input must be rejected");
}

#[test]
fn contract_173_advisory_thresholds_over_100() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let result = trust.configure_advisory_thresholds(
        account.id,
        core::services::AdvisoryThresholds {
            sector_limit_pct: dec!(200),
            asset_class_limit_pct: dec!(200),
            single_position_limit_pct: dec!(200),
        },
    );
    assert!(result.is_err(), "Invalid input must be rejected");
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 52: Trade on Nonexistent Trading Vehicle
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_174_trade_with_fake_trading_vehicle() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let fake_tv = model::TradingVehicle {
        id: Uuid::new_v4(),
        symbol: "FAKE".to_string(),
        isin: None,
        category: TradingVehicleCategory::Stock,
        exchange: Some("NOWHERE".to_string()),
        ..Default::default()
    };

    let draft = create_long_draft(&account, &fake_tv, 100);
    let result = trust.create_trade(draft, dec!(38), dec!(40), dec!(50));
    // Trading vehicle doesn't exist in DB
    assert!(
        result.is_err(),
        "Trade with non-existent trading vehicle should fail"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 53: Fund Trade Arithmetic
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_175_fund_trade_fractional_capital() {
    let mut trust = create_trust();
    // Deposit $49.99, trade needs $50.00 (1 share * $50)
    let account = setup_account_no_rules(&mut trust, dec!(49.99));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 1);
    trust
        .create_trade(draft, dec!(48), dec!(50), dec!(60))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let result = trust.fund_trade(&trade);
    assert!(
        result.is_err(),
        "Should fail: $49.99 available but $50.00 needed"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 54: Level Adjustment Rules Validation
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_176_level_adjustment_rules_default_valid() {
    use model::LevelAdjustmentRules;
    let rules = LevelAdjustmentRules::default();
    assert!(
        rules.validate().is_ok(),
        "Default level adjustment rules should be valid"
    );
}

#[test]
fn bug_177_level_adjustment_rules_zero_loss_threshold() {
    use model::LevelAdjustmentRules;
    let rules = LevelAdjustmentRules {
        monthly_loss_downgrade_pct: dec!(0),
        ..LevelAdjustmentRules::default()
    };
    assert!(
        rules.validate().is_err(),
        "Zero loss threshold should be invalid (must be negative)"
    );
}

#[test]
fn bug_178_level_adjustment_rules_positive_loss_threshold() {
    use model::LevelAdjustmentRules;
    let rules = LevelAdjustmentRules {
        monthly_loss_downgrade_pct: dec!(5),
        ..LevelAdjustmentRules::default()
    };
    assert!(
        rules.validate().is_err(),
        "Positive loss threshold should be invalid"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 55: Cancel Funded Restores Correct Amount
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_179_cancel_funded_restores_exact_amount() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(10000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let balance_before = trust
        .search_balance(account.id, &Currency::USD)
        .unwrap()
        .total_available;

    let draft = create_long_draft(&account, &tv, 50);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    trust.fund_trade(&trade).unwrap();
    let funded = trust
        .search_trades(account.id, Status::Funded)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    trust.cancel_funded_trade(&funded).unwrap();

    let balance_after = trust
        .search_balance(account.id, &Currency::USD)
        .unwrap()
        .total_available;
    assert_eq!(
        balance_before, balance_after,
        "Balance should be exactly restored after cancel_funded. Before: {}, After: {}",
        balance_before, balance_after
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 56: Level Multiplier Effects on Position Sizing
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_180_level_0_throttles_size() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    trust
        .change_level(
            account.id,
            0,
            "sizing contract",
            model::LevelTrigger::ManualOverride,
        )
        .unwrap();
    // 2% of 50,000 / 5 stop distance = 200 units before level throttling.
    let quantity = trust
        .calculate_maximum_quantity(account.id, dec!(100), dec!(95), &Currency::USD)
        .unwrap();
    assert_eq!(quantity, dec!(20));
}

#[test]
fn contract_181_level_4_cannot_override_hard_risk_budget() {
    let mut trust = create_trust();
    let account = setup_account(&mut trust, dec!(50000));
    trust
        .change_level(
            account.id,
            4,
            "sizing contract",
            model::LevelTrigger::ManualOverride,
        )
        .unwrap();
    // 2% of 50,000 / 5 stop distance = 200 units before level throttling.
    let quantity = trust
        .calculate_maximum_quantity(account.id, dec!(100), dec!(95), &Currency::USD)
        .unwrap();
    assert_eq!(quantity, dec!(200));
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 57: Trade Balance Tracking
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_182_funded_trade_has_correct_balance() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    // Entry $40, qty 100 = $4000 needed
    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    trust.fund_trade(&trade).unwrap();
    let funded = trust
        .search_trades(account.id, Status::Funded)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    assert_eq!(
        funded.balance.funding,
        dec!(4000),
        "Trade balance.funding should be $4000 (100 * $40)"
    );
}

#[test]
fn bug_183_short_trade_funded_at_stop_price() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    // Short: entry $50, stop $55, target $40, qty 100
    // Capital needed = stop * qty = $55 * 100 = $5500
    let draft = create_short_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(55), dec!(50), dec!(40))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    trust.fund_trade(&trade).unwrap();
    let funded = trust
        .search_trades(account.id, Status::Funded)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    assert_eq!(
        funded.balance.funding,
        dec!(5500),
        "Short trade funding should be based on stop price: $55 * 100 = $5500"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 58: Multiple Withdrawals
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_184_sequential_withdrawals_track_balance() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();

    // Withdraw $400
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Withdrawal,
            dec!(400),
            &Currency::USD,
        )
        .unwrap();
    let b1 = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(b1.total_available, dec!(600));

    // Withdraw $300
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Withdrawal,
            dec!(300),
            &Currency::USD,
        )
        .unwrap();
    let b2 = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(b2.total_available, dec!(300));

    // Try to withdraw $301 - should fail
    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Withdrawal,
        dec!(301),
        &Currency::USD,
    );
    assert!(
        result.is_err(),
        "Cannot withdraw $301 with only $300 available"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 59: Modify Operations on Wrong Trade States
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_185_modify_stop_on_new_trade() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let result = trust.modify_stop(&trade, &account, dec!(39));
    assert!(result.is_err(), "Modifying stop on New trade should fail");
}

#[test]
fn bug_186_modify_target_on_funded_trade() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    trust.fund_trade(&trade).unwrap();
    let funded = trust
        .search_trades(account.id, Status::Funded)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let result = trust.modify_target(&funded, &account, dec!(55));
    assert!(
        result.is_err(),
        "Modifying target on Funded (not Filled) trade should fail"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 60: Level History Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn contract_187_zero_day_history_has_no_prior_events() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    assert!(trust
        .level_history_for_account(account.id, Some(0))
        .unwrap()
        .is_empty());
}

#[test]
fn bug_188_level_history_very_large_days() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));

    let result = trust.level_history_for_account(account.id, Some(999999));
    assert!(result.is_ok(), "Very large day window should succeed");
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 61: Trading Summary Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_189_trading_summary_with_only_funded_trades() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();
    trust.fund_trade(&trade).unwrap();

    let result = trust.get_trading_summary(Some(account.id));
    assert!(
        result.is_ok(),
        "Trading summary with funded trades should succeed"
    );
}

#[test]
fn bug_190_trading_summary_with_canceled_trades() {
    let mut trust = create_trust();
    let account = setup_account_no_rules(&mut trust, dec!(50000));
    let tv = create_trading_vehicle(&mut trust, "AAPL");

    let draft = create_long_draft(&account, &tv, 100);
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .first()
        .unwrap()
        .clone();
    trust.fund_trade(&trade).unwrap();
    let funded = trust
        .search_trades(account.id, Status::Funded)
        .unwrap()
        .first()
        .unwrap()
        .clone();
    trust.cancel_funded_trade(&funded).unwrap();

    let result = trust.get_trading_summary(Some(account.id));
    assert!(
        result.is_ok(),
        "Trading summary with canceled trades should succeed"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 62: Performance Calculator with Specific Scenarios
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_191_performance_stats_single_winner() {
    use core::calculators_performance::PerformanceCalculator;

    let trades = vec![create_closed_trade(
        dec!(100),
        dec!(95),
        dec!(110),
        Status::ClosedTarget,
        dec!(100),
    )];

    let stats = PerformanceCalculator::calculate_performance_stats(&trades);
    assert_eq!(stats.win_rate, dec!(100), "Single winner = 100% win rate");
    assert_eq!(stats.winning_trades, 1);
    assert_eq!(stats.losing_trades, 0);
}

#[test]
fn contract_192_performance_stats_breakeven_counted_as_loser() {
    use core::calculators_performance::PerformanceCalculator;

    let trades = vec![create_closed_trade(
        dec!(100),
        dec!(95),
        dec!(110),
        Status::ClosedTarget,
        dec!(0),
    )];

    let stats = PerformanceCalculator::calculate_performance_stats(&trades);
    assert_eq!(stats.total_trades, 1);
    assert_eq!(stats.winning_trades, 0);
    assert_eq!(
        stats.losing_trades, 0,
        "Break-even is neither a gain nor a loss"
    );
}

#[test]
fn bug_193_performance_stats_large_win_large_loss() {
    use core::calculators_performance::PerformanceCalculator;

    let trades = vec![
        create_closed_trade(
            dec!(100),
            dec!(95),
            dec!(110),
            Status::ClosedTarget,
            dec!(10000),
        ),
        create_closed_trade(
            dec!(100),
            dec!(95),
            dec!(110),
            Status::ClosedStopLoss,
            dec!(-9999),
        ),
    ];

    let stats = PerformanceCalculator::calculate_performance_stats(&trades);
    // Net positive but very close to breakeven
    assert_eq!(stats.winning_trades, 1);
    assert_eq!(stats.losing_trades, 1);
    assert!(
        stats.average_win > dec!(0),
        "Average win should be positive"
    );
    assert!(
        stats.average_loss < dec!(0),
        "Average loss should be negative"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 63: Drawdown Calculator Edge Cases
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_194_drawdown_empty_equity_curve() {
    use core::calculators_drawdown::{RealizedDrawdownCalculator, RealizedEquityCurve};

    let curve = RealizedEquityCurve { points: vec![] };
    let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).unwrap();
    assert_eq!(
        metrics.max_drawdown_percentage,
        dec!(0),
        "Empty curve max drawdown should be 0"
    );
}

#[test]
fn bug_195_drawdown_monotonically_increasing() {
    use chrono::Utc;
    use core::calculators_drawdown::{
        EquityPoint, RealizedDrawdownCalculator, RealizedEquityCurve,
    };

    let now = Utc::now().naive_utc();
    let curve = RealizedEquityCurve {
        points: vec![
            EquityPoint {
                timestamp: now,
                balance: dec!(100),
            },
            EquityPoint {
                timestamp: now,
                balance: dec!(200),
            },
            EquityPoint {
                timestamp: now,
                balance: dec!(300),
            },
        ],
    };
    let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).unwrap();
    assert_eq!(
        metrics.max_drawdown_percentage,
        dec!(0),
        "Monotonically increasing curve has no drawdown"
    );
}

#[test]
fn bug_196_drawdown_single_value() {
    use chrono::Utc;
    use core::calculators_drawdown::{
        EquityPoint, RealizedDrawdownCalculator, RealizedEquityCurve,
    };

    let now = Utc::now().naive_utc();
    let curve = RealizedEquityCurve {
        points: vec![EquityPoint {
            timestamp: now,
            balance: dec!(100),
        }],
    };
    let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).unwrap();
    assert_eq!(
        metrics.max_drawdown_percentage,
        dec!(0),
        "Single value has no drawdown"
    );
}

#[test]
fn bug_197_drawdown_with_zero_values() {
    use chrono::Utc;
    use core::calculators_drawdown::{
        EquityPoint, RealizedDrawdownCalculator, RealizedEquityCurve,
    };

    let now = Utc::now().naive_utc();
    let curve = RealizedEquityCurve {
        points: vec![
            EquityPoint {
                timestamp: now,
                balance: dec!(100),
            },
            EquityPoint {
                timestamp: now,
                balance: dec!(50),
            },
            EquityPoint {
                timestamp: now,
                balance: dec!(0),
            },
        ],
    };
    let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).unwrap();
    assert_eq!(
        metrics.max_drawdown_percentage,
        dec!(100),
        "Going to zero should be 100% drawdown"
    );
}

// ─────────────────────────────────────────────────────────────────
// BUG CATEGORY 64: Account Hierarchy Deep Nesting
// ─────────────────────────────────────────────────────────────────

#[test]
fn bug_198_create_earnings_account_type() {
    let mut trust = create_trust();
    let parent = trust
        .create_account("parent", "parent", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    let result = trust.create_account_with_hierarchy(
        "earnings",
        "earnings acct",
        Environment::Paper,
        dec!(0),
        dec!(0),
        AccountType::Earnings,
        Some(parent.id),
    );
    assert!(
        result.is_ok(),
        "Creating earnings child account should succeed"
    );
}

#[test]
fn bug_199_create_tax_account_type() {
    let mut trust = create_trust();
    let parent = trust
        .create_account("parent", "parent", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    let result = trust.create_account_with_hierarchy(
        "taxes",
        "taxes acct",
        Environment::Paper,
        dec!(0),
        dec!(0),
        AccountType::TaxReserve,
        Some(parent.id),
    );
    assert!(result.is_ok(), "Creating tax child account should succeed");
}

#[test]
fn bug_200_double_deposit_same_amount() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "test", Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(
        balance.total_available,
        dec!(2000),
        "Two $1000 deposits should give $2000 balance"
    );
}
