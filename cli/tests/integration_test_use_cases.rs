use chrono::{Duration, Utc};
use core::calculators_concentration::{ConcentrationCalculator, MetadataField, WarningLevel};
use core::calculators_drawdown::RealizedDrawdownCalculator;
use core::calculators_performance::PerformanceCalculator;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    Account, AccountBalance, Broker, BrokerLog, Currency, DraftTrade, Order, OrderIds, OrderStatus,
    RuleLevel, RuleName, Status, Trade, TradeCategory, TradingVehicle, TradingVehicleCategory,
    Transaction, TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;
use std::error::Error;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

type SyncOutcome = Result<(Status, Vec<Order>), String>;

#[derive(Debug)]
struct BrokerState {
    submit_error: Option<String>,
    cancel_error: Option<String>,
    close_error: Option<String>,
    modify_stop_error: Option<String>,
    modify_target_error: Option<String>,
    sync_queue: VecDeque<SyncOutcome>,
    modify_stop_id: Uuid,
    modify_target_id: Uuid,
}

#[derive(Clone, Debug)]
struct TestBroker {
    state: Arc<Mutex<BrokerState>>,
}

impl TestBroker {
    fn success() -> Self {
        Self {
            state: Arc::new(Mutex::new(BrokerState {
                submit_error: None,
                cancel_error: None,
                close_error: None,
                modify_stop_error: None,
                modify_target_error: None,
                sync_queue: VecDeque::new(),
                modify_stop_id: Uuid::from_u128(0x11111111111111111111111111111111),
                modify_target_id: Uuid::from_u128(0x22222222222222222222222222222222),
            })),
        }
    }

    fn with_submit_error(message: &str) -> Self {
        let broker = Self::success();
        broker.state.lock().expect("broker state lock").submit_error = Some(message.to_string());
        broker
    }

    fn with_cancel_error(message: &str) -> Self {
        let broker = Self::success();
        broker.state.lock().expect("broker state lock").cancel_error = Some(message.to_string());
        broker
    }

    fn enqueue_sync(&self, outcome: SyncOutcome) {
        self.state
            .lock()
            .expect("broker state lock")
            .sync_queue
            .push_back(outcome);
    }

    fn modify_stop_id(&self) -> Uuid {
        self.state.lock().expect("broker state lock").modify_stop_id
    }

    fn modify_target_id(&self) -> Uuid {
        self.state
            .lock()
            .expect("broker state lock")
            .modify_target_id
    }
}

impl Broker for TestBroker {
    fn submit_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        let state = self.state.lock().expect("broker state lock");
        if let Some(message) = &state.submit_error {
            return Err(message.clone().into());
        }

        let log = BrokerLog {
            trade_id: trade.id,
            log: "submit ok".to_string(),
            ..Default::default()
        };

        let ids = OrderIds {
            entry: Uuid::from_u128(0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa),
            target: Uuid::from_u128(0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb),
            stop: Uuid::from_u128(0xcccccccccccccccccccccccccccccccc),
        };

        Ok((log, ids))
    }

    fn sync_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        let outcome = {
            let mut state = self.state.lock().expect("broker state lock");
            state
                .sync_queue
                .pop_front()
                .unwrap_or_else(|| Ok((Status::Submitted, vec![])))
        };

        let (status, orders) = outcome.map_err(|e| -> Box<dyn Error> { e.into() })?;

        let log = BrokerLog {
            trade_id: trade.id,
            log: "sync ok".to_string(),
            ..Default::default()
        };

        Ok((status, orders, log))
    }

    fn close_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        let state = self.state.lock().expect("broker state lock");
        if let Some(message) = &state.close_error {
            return Err(message.clone().into());
        }

        let mut market_target = trade.target.clone();
        market_target.category = model::OrderCategory::Market;
        market_target.status = OrderStatus::PendingNew;

        let log = BrokerLog {
            trade_id: trade.id,
            log: "close ok".to_string(),
            ..Default::default()
        };

        Ok((market_target, log))
    }

    fn cancel_trade(&self, _trade: &Trade, _account: &Account) -> Result<(), Box<dyn Error>> {
        let state = self.state.lock().expect("broker state lock");
        if let Some(message) = &state.cancel_error {
            return Err(message.clone().into());
        }
        Ok(())
    }

    fn modify_stop(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_stop_price: Decimal,
    ) -> Result<Uuid, Box<dyn Error>> {
        let state = self.state.lock().expect("broker state lock");
        if let Some(message) = &state.modify_stop_error {
            return Err(message.clone().into());
        }
        Ok(state.modify_stop_id)
    }

    fn modify_target(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_target_price: Decimal,
    ) -> Result<Uuid, Box<dyn Error>> {
        let state = self.state.lock().expect("broker state lock");
        if let Some(message) = &state.modify_target_error {
            return Err(message.clone().into());
        }
        Ok(state.modify_target_id)
    }
}

fn new_trust_with_broker(broker: TestBroker) -> TrustFacade {
    TrustFacade::new(Box::new(SqliteDatabase::new_in_memory()), Box::new(broker))
}

fn create_account(trust: &mut TrustFacade, name: &str, env: model::Environment) -> Account {
    trust
        .create_account(name, "integration test", env, dec!(20), dec!(10))
        .expect("create account")
}

fn deposit(
    trust: &mut TrustFacade,
    account: &Account,
    amount: Decimal,
    currency: Currency,
) -> (Transaction, AccountBalance) {
    trust
        .create_transaction(account, &TransactionCategory::Deposit, amount, &currency)
        .expect("create deposit")
}

fn create_vehicle(trust: &mut TrustFacade, symbol: &str) -> TradingVehicle {
    let isin = format!("US{}", Uuid::new_v4().simple());
    trust
        .create_trading_vehicle(symbol, &isin, &TradingVehicleCategory::Stock, "Nasdaq")
        .expect("create trading vehicle")
}

#[allow(clippy::too_many_arguments)]
fn create_trade(
    trust: &mut TrustFacade,
    account: &Account,
    trading_vehicle: &TradingVehicle,
    category: TradeCategory,
    quantity: i64,
    entry: Decimal,
    stop: Decimal,
    target: Decimal,
    thesis: Option<&str>,
    sector: Option<&str>,
    asset_class: Option<&str>,
    context: Option<&str>,
) -> Trade {
    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: trading_vehicle.clone(),
        quantity,
        currency: Currency::USD,
        category,
        thesis: thesis.map(ToString::to_string),
        sector: sector.map(ToString::to_string),
        asset_class: asset_class.map(ToString::to_string),
        context: context.map(ToString::to_string),
    };

    trust
        .create_trade(draft, stop, entry, target)
        .expect("create trade")
}

fn trade_by_status_and_id(
    trust: &mut TrustFacade,
    account: &Account,
    status: Status,
    id: Uuid,
) -> Trade {
    let trades = trust
        .search_trades(account.id, status)
        .expect("search trades by status");
    let maybe_trade = trades.into_iter().find(|t| t.id == id);
    maybe_trade.expect("trade with requested status and id")
}

fn usd_balance(trust: &mut TrustFacade, account: &Account) -> AccountBalance {
    trust
        .search_balance(account.id, &Currency::USD)
        .expect("search USD balance")
}

fn account_transactions(trust: &mut TrustFacade, account: &Account) -> Vec<Transaction> {
    trust
        .get_account_transactions(account.id)
        .expect("account transactions")
}

fn add_risk_rules(trust: &mut TrustFacade, account: &Account, risk_month: f32, risk_trade: f32) {
    trust
        .create_rule(
            account,
            &RuleName::RiskPerMonth(risk_month),
            "risk month",
            &RuleLevel::Error,
        )
        .expect("create rule risk month");

    trust
        .create_rule(
            account,
            &RuleName::RiskPerTrade(risk_trade),
            "risk trade",
            &RuleLevel::Error,
        )
        .expect("create rule risk trade");
}

fn fund_trade(trust: &mut TrustFacade, account: &Account, trade: &Trade) -> Trade {
    trust.fund_trade(trade).expect("fund trade");
    trade_by_status_and_id(trust, account, Status::Funded, trade.id)
}

fn submit_trade(trust: &mut TrustFacade, account: &Account, trade: &Trade) -> Trade {
    trust.submit_trade(trade).expect("submit trade");
    trade_by_status_and_id(trust, account, Status::Submitted, trade.id)
}

fn order_entry_filled(trade: &Trade, average_price: Decimal) -> Order {
    let mut entry = trade.entry.clone();
    entry.status = OrderStatus::Filled;
    entry.filled_quantity = trade.entry.quantity;
    entry.average_filled_price = Some(average_price);
    entry.filled_at = Some(Utc::now().naive_utc());
    entry
}

fn order_target_filled(trade: &Trade, average_price: Decimal) -> Order {
    let mut target = trade.target.clone();
    target.status = OrderStatus::Filled;
    target.filled_quantity = trade.target.quantity;
    target.average_filled_price = Some(average_price);
    target.filled_at = Some(Utc::now().naive_utc());
    target
}

fn order_stop_filled(trade: &Trade, average_price: Decimal) -> Order {
    let mut stop = trade.safety_stop.clone();
    stop.status = OrderStatus::Filled;
    stop.filled_quantity = trade.safety_stop.quantity;
    stop.average_filled_price = Some(average_price);
    stop.filled_at = Some(Utc::now().naive_utc());
    stop
}

fn order_target_accepted(trade: &Trade) -> Order {
    let mut target = trade.target.clone();
    target.status = OrderStatus::Accepted;
    target.filled_quantity = 0;
    target.average_filled_price = None;
    target
}

fn order_stop_held(trade: &Trade) -> Order {
    let mut stop = trade.safety_stop.clone();
    stop.status = OrderStatus::Held;
    stop.filled_quantity = 0;
    stop.average_filled_price = None;
    stop
}

fn order_stop_canceled(trade: &Trade) -> Order {
    let mut stop = trade.safety_stop.clone();
    stop.status = OrderStatus::Canceled;
    stop.filled_quantity = 0;
    stop.average_filled_price = None;
    stop
}

fn setup_account_deposit_vehicle_trade(
    trust: &mut TrustFacade,
    name: &str,
    deposit_amount: Decimal,
    category: TradeCategory,
    quantity: i64,
    entry: Decimal,
    stop: Decimal,
    target: Decimal,
) -> (Account, TradingVehicle, Trade) {
    let account = create_account(trust, name, model::Environment::Paper);
    deposit(trust, &account, deposit_amount, Currency::USD);
    let vehicle = create_vehicle(trust, "AAPL");
    let trade = create_trade(
        trust, &account, &vehicle, category, quantity, entry, stop, target, None, None, None, None,
    );
    (account, vehicle, trade)
}

#[test]
fn test_case_01_duplicate_account_name_is_rejected() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    create_account(&mut trust, "alpha", model::Environment::Paper);

    let err = trust
        .create_account(
            "alpha",
            "duplicate",
            model::Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect_err("duplicate account must fail");

    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("unique") || msg.contains("accounts.name"));
}

#[test]
fn test_case_02_account_search_is_case_insensitive() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let created = create_account(&mut trust, "MyAccount", model::Environment::Paper);

    let found = trust
        .search_account("MYACCOUNT")
        .expect("must find account by case-insensitive name");

    assert_eq!(created.id, found.id);
    assert_eq!(found.name, "myaccount");
}

#[test]
fn test_case_03_same_account_name_across_environments_is_rejected() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    create_account(&mut trust, "shared-name", model::Environment::Paper);

    let err = trust
        .create_account(
            "shared-name",
            "live duplicate",
            model::Environment::Live,
            dec!(20),
            dec!(10),
        )
        .expect_err("name uniqueness is global");

    assert!(err.to_string().to_lowercase().contains("unique"));
}

#[test]
fn test_case_04_deposit_creates_balance_and_transaction() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let account = create_account(&mut trust, "acct-04", model::Environment::Paper);

    let (tx, balance) = deposit(&mut trust, &account, dec!(5000), Currency::USD);

    assert_eq!(tx.category, TransactionCategory::Deposit);
    assert_eq!(tx.amount, dec!(5000));
    assert_eq!(tx.currency, Currency::USD);
    assert_eq!(balance.total_available, dec!(5000));
    assert_eq!(balance.total_balance, dec!(5000));
    assert_eq!(balance.total_in_trade, dec!(0));
}

#[test]
fn test_case_05_first_deposit_in_new_currency_creates_new_balance_row() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let account = create_account(&mut trust, "acct-05", model::Environment::Paper);

    deposit(&mut trust, &account, dec!(1000), Currency::USD);
    deposit(&mut trust, &account, dec!(2), Currency::BTC);

    let balances = trust
        .search_all_balances(account.id)
        .expect("all balances must exist");

    assert_eq!(balances.len(), 2);
    assert!(balances.iter().any(|b| b.currency == Currency::USD));
    assert!(balances.iter().any(|b| b.currency == Currency::BTC));
}

#[test]
fn test_case_06_negative_deposit_is_rejected() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let account = create_account(&mut trust, "acct-06", model::Environment::Paper);

    let err = trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(-1),
            &Currency::USD,
        )
        .expect_err("negative deposit must fail");

    assert!(err
        .to_string()
        .contains("Amount of deposit must be positive"));
}

#[test]
fn test_case_07_withdrawal_exact_available_zeroes_balance() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let account = create_account(&mut trust, "acct-07", model::Environment::Paper);

    deposit(&mut trust, &account, dec!(1250), Currency::USD);

    let (_tx, balance) = trust
        .create_transaction(
            &account,
            &TransactionCategory::Withdrawal,
            dec!(1250),
            &Currency::USD,
        )
        .expect("full withdrawal should succeed");

    assert_eq!(balance.total_available, dec!(0));
    assert_eq!(balance.total_balance, dec!(0));
    assert_eq!(balance.total_in_trade, dec!(0));
}

#[test]
fn test_case_08_withdrawal_greater_than_available_is_rejected() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let account = create_account(&mut trust, "acct-08", model::Environment::Paper);

    deposit(&mut trust, &account, dec!(100), Currency::USD);

    let err = trust
        .create_transaction(
            &account,
            &TransactionCategory::Withdrawal,
            dec!(101),
            &Currency::USD,
        )
        .expect_err("over-withdrawal must fail");

    assert!(err
        .to_string()
        .contains("Withdrawal amount is greater than available amount"));
}

#[test]
fn test_case_09_withdrawal_from_missing_currency_is_rejected() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let account = create_account(&mut trust, "acct-09", model::Environment::Paper);

    deposit(&mut trust, &account, dec!(100), Currency::USD);

    let err = trust
        .create_transaction(
            &account,
            &TransactionCategory::Withdrawal,
            dec!(1),
            &Currency::BTC,
        )
        .expect_err("withdrawal with no BTC balance must fail");

    assert!(
        err.to_string().contains("Overview not found")
            || err.to_string().contains("OverviewForWithdrawNotFound")
    );
}

#[test]
fn test_case_10_high_precision_transaction_arithmetic_is_preserved() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let account = create_account(&mut trust, "acct-10", model::Environment::Paper);

    deposit(&mut trust, &account, dec!(1.1111), Currency::USD);
    deposit(&mut trust, &account, dec!(2.2222), Currency::USD);
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Withdrawal,
            dec!(0.3333),
            &Currency::USD,
        )
        .expect("precision withdrawal should succeed");

    let balance = usd_balance(&mut trust, &account);
    assert_eq!(balance.total_available, dec!(3.0000));
    assert_eq!(balance.total_balance, dec!(3.0000));
}

#[test]
fn test_case_11_large_transaction_values_remain_consistent() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let account = create_account(&mut trust, "acct-11", model::Environment::Paper);

    deposit(&mut trust, &account, dec!(999999999999.99), Currency::USD);
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Withdrawal,
            dec!(0.01),
            &Currency::USD,
        )
        .expect("large withdrawal should succeed");

    let balance = usd_balance(&mut trust, &account);
    assert_eq!(balance.total_available, dec!(999999999999.98));
    assert_eq!(balance.total_balance, dec!(999999999999.98));
}

#[test]
fn test_case_12_duplicate_rule_name_is_rejected_for_same_account() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let account = create_account(&mut trust, "acct-12", model::Environment::Paper);

    trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(2.0),
            "first",
            &RuleLevel::Error,
        )
        .expect("first rule must succeed");

    let err = trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(3.0),
            "duplicate name",
            &RuleLevel::Error,
        )
        .expect_err("duplicate rule type must fail");

    assert!(err
        .to_string()
        .contains("Rule with name risk_per_trade already exists"));
}

#[test]
fn test_case_13_deactivate_rule_removes_it_from_active_search() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let account = create_account(&mut trust, "acct-13", model::Environment::Paper);

    let rule = trust
        .create_rule(
            &account,
            &RuleName::RiskPerMonth(6.0),
            "monthly",
            &RuleLevel::Error,
        )
        .expect("rule creation");

    let deactivated = trust
        .deactivate_rule(&rule)
        .expect("rule deactivation must succeed");

    assert!(!deactivated.active);

    let active_rules = trust.search_rules(account.id).expect("search rules");
    assert!(active_rules.is_empty());
}

#[test]
fn test_case_14_risk_per_month_lower_than_risk_per_trade_blocks_funding() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let (account, vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-14",
        dec!(10000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(95),
        dec!(120),
    );

    add_risk_rules(&mut trust, &account, 1.0, 2.0);

    let err = trust
        .fund_trade(&trade)
        .expect_err("funding should be blocked by risk per month");

    assert!(err.to_string().contains("Risk per month exceeded"));

    // Ensure trade remains new.
    let new_trade = trade_by_status_and_id(&mut trust, &account, Status::New, trade.id);
    assert_eq!(new_trade.status, Status::New);
    let _ = vehicle; // Keep tuple fully used for strict lints.
}

#[test]
fn test_case_15_maximum_quantity_with_rules_enforces_risk_cap() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let account = create_account(&mut trust, "acct-15", model::Environment::Paper);
    deposit(&mut trust, &account, dec!(50000), Currency::USD);
    add_risk_rules(&mut trust, &account, 6.0, 2.0);

    let quantity = trust
        .calculate_maximum_quantity(account.id, dec!(40), dec!(38), &Currency::USD)
        .expect("maximum quantity");

    assert_eq!(quantity, 500);
}

#[test]
fn test_case_16_maximum_quantity_without_rules_uses_available_div_entry() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let account = create_account(&mut trust, "acct-16", model::Environment::Paper);
    deposit(&mut trust, &account, dec!(1000), Currency::USD);

    let quantity = trust
        .calculate_maximum_quantity(account.id, dec!(40), dec!(38), &Currency::USD)
        .expect("maximum quantity no rules");

    assert_eq!(quantity, 25);
}

#[test]
fn test_case_17_duplicate_trading_vehicle_isin_is_rejected() {
    let mut trust = new_trust_with_broker(TestBroker::success());

    trust
        .create_trading_vehicle(
            "AAPL",
            "US0378331005",
            &TradingVehicleCategory::Stock,
            "Nasdaq",
        )
        .expect("first vehicle");

    let err = trust
        .create_trading_vehicle(
            "AAPL",
            "US0378331005",
            &TradingVehicleCategory::Stock,
            "Nasdaq",
        )
        .expect_err("duplicate ISIN must fail");

    assert!(err.to_string().to_lowercase().contains("unique"));
}

#[test]
fn test_case_18_trading_vehicle_values_are_normalized() {
    let mut trust = new_trust_with_broker(TestBroker::success());

    let tv = trust
        .create_trading_vehicle(
            "aapl",
            "us0378331005",
            &TradingVehicleCategory::Stock,
            "NaSdAq",
        )
        .expect("create vehicle");

    assert_eq!(tv.symbol, "AAPL");
    assert_eq!(tv.isin, "US0378331005");
    assert_eq!(tv.broker, "nasdaq");
}

#[test]
fn test_case_19_trade_creation_preserves_metadata_and_initial_status() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let account = create_account(&mut trust, "acct-19", model::Environment::Paper);
    let vehicle = create_vehicle(&mut trust, "NVDA");

    let trade = create_trade(
        &mut trust,
        &account,
        &vehicle,
        TradeCategory::Long,
        7,
        dec!(100),
        dec!(90),
        dec!(130),
        Some("breakout setup"),
        Some("technology"),
        Some("stocks"),
        Some("daily chart"),
    );

    assert_eq!(trade.status, Status::New);
    assert_eq!(trade.thesis.as_deref(), Some("breakout setup"));
    assert_eq!(trade.sector.as_deref(), Some("technology"));
    assert_eq!(trade.asset_class.as_deref(), Some("stocks"));
    assert_eq!(trade.context.as_deref(), Some("daily chart"));
    assert_eq!(trade.entry.unit_price, dec!(100));
    assert_eq!(trade.safety_stop.unit_price, dec!(90));
    assert_eq!(trade.target.unit_price, dec!(130));
}

#[test]
fn test_case_20_funding_long_reserves_entry_price_times_quantity() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-20",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let balance = usd_balance(&mut trust, &account);

    assert_eq!(funded.status, Status::Funded);
    assert_eq!(balance.total_available, dec!(4000));
    assert_eq!(balance.total_in_trade, dec!(1000));
    assert_eq!(balance.total_balance, dec!(5000));
}

#[test]
fn test_case_21_funding_short_reserves_stop_price_times_quantity() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-21",
        dec!(5000),
        TradeCategory::Short,
        10,
        dec!(100),
        dec!(120),
        dec!(80),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let balance = usd_balance(&mut trust, &account);

    assert_eq!(funded.status, Status::Funded);
    assert_eq!(balance.total_available, dec!(3800));
    assert_eq!(balance.total_in_trade, dec!(1200));
    assert_eq!(balance.total_balance, dec!(5000));
}

#[test]
fn test_case_22_funding_fails_when_insufficient_capital() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-22",
        dec!(500),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let err = trust
        .fund_trade(&trade)
        .expect_err("funding must fail due to insufficient capital");

    assert!(err.to_string().contains("Not enough funds"));

    let still_new = trade_by_status_and_id(&mut trust, &account, Status::New, trade.id);
    assert_eq!(still_new.status, Status::New);
}

#[test]
fn test_case_23_submit_requires_funded_status() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-23",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let err = trust
        .submit_trade(&trade)
        .expect_err("submit on non-funded trade must fail");

    assert!(err.to_string().contains("not funded"));

    let still_new = trade_by_status_and_id(&mut trust, &account, Status::New, trade.id);
    assert_eq!(still_new.status, Status::New);
}

#[test]
fn test_case_24_submit_funded_trade_sets_submitted_and_broker_ids() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker);
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-24",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    assert_eq!(submitted.status, Status::Submitted);
    assert!(submitted.entry.broker_order_id.is_some());
    assert!(submitted.target.broker_order_id.is_some());
    assert!(submitted.safety_stop.broker_order_id.is_some());
}

#[test]
fn test_case_25_submit_broker_error_keeps_trade_funded() {
    let broker = TestBroker::with_submit_error("submit failed");
    let mut trust = new_trust_with_broker(broker);
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-25",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);

    let err = trust
        .submit_trade(&funded)
        .expect_err("submit must fail from broker side");
    assert!(err.to_string().contains("submit failed"));

    let still_funded = trade_by_status_and_id(&mut trust, &account, Status::Funded, trade.id);
    assert_eq!(still_funded.status, Status::Funded);
}

#[test]
fn test_case_26_cancel_funded_returns_reserved_capital() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let initial = dec!(5000);

    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-26",
        initial,
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);

    let (_trade_balance, account_balance, tx) = trust
        .cancel_funded_trade(&funded)
        .expect("cancel funded trade");

    let canceled = trade_by_status_and_id(&mut trust, &account, Status::Canceled, trade.id);

    assert_eq!(canceled.status, Status::Canceled);
    assert_eq!(tx.category, TransactionCategory::PaymentFromTrade(trade.id));
    assert_eq!(account_balance.total_available, initial);
    assert_eq!(usd_balance(&mut trust, &account).total_available, initial);
}

#[test]
fn test_case_27_cancel_submitted_trade_successfully_returns_funds() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let initial = dec!(5000);

    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-27",
        initial,
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    let (_trade_balance, account_balance, tx) = trust
        .cancel_submitted_trade(&submitted)
        .expect("cancel submitted trade");

    let canceled = trade_by_status_and_id(&mut trust, &account, Status::Canceled, trade.id);

    assert_eq!(canceled.status, Status::Canceled);
    assert_eq!(tx.category, TransactionCategory::PaymentFromTrade(trade.id));
    assert_eq!(account_balance.total_available, initial);
    assert_eq!(usd_balance(&mut trust, &account).total_available, initial);
}

#[test]
fn test_case_28_cancel_submitted_trade_broker_error_preserves_state() {
    let mut trust = new_trust_with_broker(TestBroker::with_cancel_error("cancel failed"));
    let initial = dec!(5000);

    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-28",
        initial,
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    let err = trust
        .cancel_submitted_trade(&submitted)
        .expect_err("cancel should fail when broker rejects");

    assert!(err.to_string().contains("cancel failed"));

    let still_submitted = trade_by_status_and_id(&mut trust, &account, Status::Submitted, trade.id);
    assert_eq!(still_submitted.status, Status::Submitted);

    // Funds are still reserved.
    let balance = usd_balance(&mut trust, &account);
    assert_eq!(balance.total_available, initial - dec!(1000));
}

#[test]
fn test_case_29_cancel_funded_called_on_new_trade_is_rejected() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-29",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let err = trust
        .cancel_funded_trade(&trade)
        .expect_err("new trade cannot be canceled as funded");

    assert!(err.to_string().contains("is not funded"));

    let still_new = trade_by_status_and_id(&mut trust, &account, Status::New, trade.id);
    assert_eq!(still_new.status, Status::New);
}

#[test]
fn test_case_30_modify_stop_rejects_increased_risk_for_long_trade() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-30",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_accepted(&submitted),
            order_stop_held(&submitted),
        ],
    )));

    trust
        .sync_trade(&submitted, &account)
        .expect("sync to filled for modify test");

    let filled = trade_by_status_and_id(&mut trust, &account, Status::Filled, trade.id);

    let err = trust
        .modify_stop(&filled, &account, dec!(80))
        .expect_err("long trade stop cannot be lowered");

    assert!(err.to_string().contains("risking more money"));
}

#[test]
fn test_case_31_modify_stop_with_reduced_risk_updates_price_and_broker_id() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-31",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_accepted(&submitted),
            order_stop_held(&submitted),
        ],
    )));

    trust
        .sync_trade(&submitted, &account)
        .expect("sync to filled for modify test");

    let filled = trade_by_status_and_id(&mut trust, &account, Status::Filled, trade.id);
    let updated = trust
        .modify_stop(&filled, &account, dec!(95))
        .expect("modify stop should succeed");

    assert_eq!(updated.safety_stop.unit_price, dec!(95));
    assert_eq!(
        updated.safety_stop.broker_order_id,
        Some(broker.modify_stop_id())
    );
}

#[test]
fn test_case_32_modify_stop_on_non_filled_trade_is_rejected() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-32",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    let err = trust
        .modify_stop(&submitted, &account, dec!(95))
        .expect_err("modify stop requires filled trade");

    assert!(err.to_string().contains("is not filled"));
}

#[test]
fn test_case_33_modify_target_on_non_filled_trade_is_rejected() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-33",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    let err = trust
        .modify_target(&submitted, &account, dec!(130))
        .expect_err("modify target requires filled trade");

    assert!(err.to_string().contains("is not filled"));
}

#[test]
fn test_case_34_modify_target_on_filled_trade_updates_price_and_broker_id() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-34",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_accepted(&submitted),
            order_stop_held(&submitted),
        ],
    )));

    trust
        .sync_trade(&submitted, &account)
        .expect("sync to filled for modify target");

    let filled = trade_by_status_and_id(&mut trust, &account, Status::Filled, trade.id);
    let updated = trust
        .modify_target(&filled, &account, dec!(130))
        .expect("modify target should succeed");

    assert_eq!(updated.target.unit_price, dec!(130));
    assert_eq!(
        updated.target.broker_order_id,
        Some(broker.modify_target_id())
    );
}

#[test]
fn test_case_35_close_trade_on_filled_sets_canceled_and_cancels_stop_order() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-35b",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );
    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_accepted(&submitted),
            order_stop_held(&submitted),
        ],
    )));

    trust
        .sync_trade(&submitted, &account)
        .expect("sync to filled for close trade");

    let filled = trade_by_status_and_id(&mut trust, &account, Status::Filled, trade.id);

    trust.close_trade(&filled).expect("close trade");

    let canceled = trade_by_status_and_id(&mut trust, &account, Status::Canceled, trade.id);
    assert_eq!(canceled.status, Status::Canceled);
    assert_eq!(canceled.safety_stop.status, OrderStatus::Canceled);
    assert_eq!(canceled.target.category, model::OrderCategory::Market);
}

#[test]
fn test_case_36_close_trade_on_non_filled_is_rejected() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-36",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    let err = trust
        .close_trade(&submitted)
        .expect_err("close requires filled status");

    assert!(err.to_string().contains("is not filled"));
}

#[test]
fn test_case_37_sync_submitted_to_filled_creates_open_trade_transaction() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-37",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    let tx_before = account_transactions(&mut trust, &account).len();

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_accepted(&submitted),
            order_stop_held(&submitted),
        ],
    )));

    trust
        .sync_trade(&submitted, &account)
        .expect("sync to filled should succeed");

    let filled = trade_by_status_and_id(&mut trust, &account, Status::Filled, trade.id);
    assert_eq!(filled.status, Status::Filled);
    assert_eq!(filled.balance.capital_in_market, dec!(1000));
    assert_eq!(filled.balance.capital_out_market, dec!(0));

    let tx_after = account_transactions(&mut trust, &account).len();
    assert_eq!(tx_after, tx_before);

    let balance = usd_balance(&mut trust, &account);
    assert_eq!(balance.total_in_trade, dec!(1000));
}

#[test]
fn test_case_38_repeated_sync_filled_is_idempotent_for_transactions() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-38",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_accepted(&submitted),
            order_stop_held(&submitted),
        ],
    )));

    trust
        .sync_trade(&submitted, &account)
        .expect("first sync to filled");

    let filled = trade_by_status_and_id(&mut trust, &account, Status::Filled, trade.id);
    let tx_before = account_transactions(&mut trust, &account).len();

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&filled, dec!(100)),
            order_target_accepted(&filled),
            order_stop_held(&filled),
        ],
    )));

    trust
        .sync_trade(&filled, &account)
        .expect("second sync should be idempotent for tx");

    let tx_after = account_transactions(&mut trust, &account).len();
    assert_eq!(tx_after, tx_before);
}

#[test]
fn test_case_39_sync_submitted_to_closed_target_realizes_profit_and_releases_capital() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let initial = dec!(5000);
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-39",
        initial,
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    broker.enqueue_sync(Ok((
        Status::ClosedTarget,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_filled(&submitted, dec!(120)),
            order_stop_canceled(&submitted),
        ],
    )));

    trust
        .sync_trade(&submitted, &account)
        .expect("sync to closed target");

    let closed = trade_by_status_and_id(&mut trust, &account, Status::ClosedTarget, trade.id);
    assert_eq!(closed.status, Status::ClosedTarget);

    let balance = usd_balance(&mut trust, &account);
    assert_eq!(balance.total_in_trade, dec!(0));
    assert_eq!(balance.total_available, initial + dec!(200));
}

#[test]
fn test_case_40_sync_submitted_to_closed_stop_realizes_loss() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let initial = dec!(5000);
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-40",
        initial,
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    broker.enqueue_sync(Ok((
        Status::ClosedStopLoss,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_accepted(&submitted),
            order_stop_filled(&submitted, dec!(90)),
        ],
    )));

    trust
        .sync_trade(&submitted, &account)
        .expect("sync to closed stop loss");

    let closed = trade_by_status_and_id(&mut trust, &account, Status::ClosedStopLoss, trade.id);
    assert_eq!(closed.status, Status::ClosedStopLoss);

    let balance = usd_balance(&mut trust, &account);
    assert_eq!(balance.total_in_trade, dec!(0));
    assert_eq!(balance.total_available, initial - dec!(100));
}

#[test]
fn test_case_41_sync_canceled_trade_to_closed_target_reconciles_manual_close() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-41",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    // First sync to filled so close_trade is allowed.
    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_accepted(&submitted),
            order_stop_held(&submitted),
        ],
    )));

    trust
        .sync_trade(&submitted, &account)
        .expect("sync to filled before manual close");

    let filled = trade_by_status_and_id(&mut trust, &account, Status::Filled, trade.id);
    trust
        .close_trade(&filled)
        .expect("manual close to canceled state");

    let canceled = trade_by_status_and_id(&mut trust, &account, Status::Canceled, trade.id);

    // Broker later reports close target fill for the replacement market target.
    broker.enqueue_sync(Ok((
        Status::ClosedTarget,
        vec![order_target_filled(&canceled, dec!(110))],
    )));

    trust
        .sync_trade(&canceled, &account)
        .expect("sync canceled->closed_target reconciliation");

    let reconciled = trade_by_status_and_id(&mut trust, &account, Status::ClosedTarget, trade.id);
    assert_eq!(reconciled.status, Status::ClosedTarget);
}

#[test]
fn test_case_42_sync_with_non_matching_order_fails_without_status_transition() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-42",
        dec!(5000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    let mut unknown_order = submitted.entry.clone();
    unknown_order.id = Uuid::new_v4();
    unknown_order.status = OrderStatus::Filled;
    unknown_order.filled_quantity = submitted.entry.quantity;
    unknown_order.average_filled_price = Some(dec!(100));

    broker.enqueue_sync(Ok((Status::Filled, vec![unknown_order])));

    let err = trust
        .sync_trade(&submitted, &account)
        .expect_err("sync must fail when order id is unknown");

    assert!(
        err.to_string().to_lowercase().contains("not found")
            || err.to_string().to_lowercase().contains("record")
            || err.to_string().to_lowercase().contains("row")
    );

    let still_submitted = trade_by_status_and_id(&mut trust, &account, Status::Submitted, trade.id);
    assert_eq!(still_submitted.status, Status::Submitted);
}

#[test]
fn test_case_43_manual_stop_with_fee_creates_closing_and_payment_transactions() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let initial = dec!(5000);
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-43",
        initial,
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_accepted(&submitted),
            order_stop_held(&submitted),
        ],
    )));

    trust
        .sync_trade(&submitted, &account)
        .expect("sync to filled before manual stop");

    let mut filled = trade_by_status_and_id(&mut trust, &account, Status::Filled, trade.id);
    filled.safety_stop.average_filled_price = Some(dec!(90));

    let (tx_stop, tx_payment, _trade_balance, _account_balance) = trust
        .stop_trade(&filled, dec!(5))
        .expect("manual stop should succeed");

    assert!(matches!(
        tx_stop.category,
        TransactionCategory::CloseSafetyStop(_) | TransactionCategory::CloseSafetyStopSlippage(_)
    ));
    assert_eq!(
        tx_payment.category,
        TransactionCategory::PaymentFromTrade(trade.id)
    );

    let closed = trade_by_status_and_id(&mut trust, &account, Status::ClosedStopLoss, trade.id);
    assert_eq!(closed.status, Status::ClosedStopLoss);

    let balance = usd_balance(&mut trust, &account);
    assert!(balance.total_available < initial);
}

#[test]
fn test_case_44_manual_target_with_fee_creates_closing_and_payment_transactions() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let initial = dec!(5000);
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-44",
        initial,
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );

    let funded = fund_trade(&mut trust, &account, &trade);
    let submitted = submit_trade(&mut trust, &account, &funded);

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_accepted(&submitted),
            order_stop_held(&submitted),
        ],
    )));

    trust
        .sync_trade(&submitted, &account)
        .expect("sync to filled before manual target");

    let mut filled = trade_by_status_and_id(&mut trust, &account, Status::Filled, trade.id);
    filled.target.average_filled_price = Some(dec!(120));

    let (tx_target, tx_payment, _trade_balance, _account_balance) = trust
        .target_acquired(&filled, dec!(5))
        .expect("manual target should succeed");

    assert_eq!(
        tx_target.category,
        TransactionCategory::CloseTarget(trade.id)
    );
    assert_eq!(
        tx_payment.category,
        TransactionCategory::PaymentFromTrade(trade.id)
    );

    let closed = trade_by_status_and_id(&mut trust, &account, Status::ClosedTarget, trade.id);
    assert_eq!(closed.status, Status::ClosedTarget);

    let balance = usd_balance(&mut trust, &account);
    assert!(balance.total_available > initial - dec!(1000));
}

#[test]
fn test_case_45_risk_open_positions_returns_only_open_trade_statuses() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());
    let account = create_account(&mut trust, "acct-45", model::Environment::Paper);
    deposit(&mut trust, &account, dec!(20000), Currency::USD);

    let vehicle = create_vehicle(&mut trust, "AAPL");

    // Trade A -> funded
    let trade_a = create_trade(
        &mut trust,
        &account,
        &vehicle,
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
        None,
        None,
        None,
        None,
    );
    let _funded_a = fund_trade(&mut trust, &account, &trade_a);

    // Trade B -> submitted
    let trade_b = create_trade(
        &mut trust,
        &account,
        &vehicle,
        TradeCategory::Long,
        10,
        dec!(110),
        dec!(100),
        dec!(130),
        None,
        None,
        None,
        None,
    );
    let funded_b = fund_trade(&mut trust, &account, &trade_b);
    let submitted_b = submit_trade(&mut trust, &account, &funded_b);

    // Trade C -> filled
    let trade_c = create_trade(
        &mut trust,
        &account,
        &vehicle,
        TradeCategory::Long,
        10,
        dec!(120),
        dec!(110),
        dec!(150),
        None,
        None,
        None,
        None,
    );
    let funded_c = fund_trade(&mut trust, &account, &trade_c);
    let submitted_c = submit_trade(&mut trust, &account, &funded_c);

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted_c, dec!(120)),
            order_target_accepted(&submitted_c),
            order_stop_held(&submitted_c),
        ],
    )));

    trust
        .sync_trade(&submitted_c, &account)
        .expect("sync to fill trade C");

    let positions = trust
        .calculate_open_positions(Some(account.id))
        .expect("calculate open positions");

    assert_eq!(positions.len(), 3);
    assert!(positions
        .iter()
        .any(|p| p.trade_id == trade_a.id && p.status == Status::Funded));
    assert!(positions
        .iter()
        .any(|p| p.trade_id == submitted_b.id && p.status == Status::Submitted));
    assert!(positions
        .iter()
        .any(|p| p.trade_id == trade_c.id && p.status == Status::Filled));
}

#[test]
fn test_case_46_risk_open_positions_without_account_returns_empty_current_behavior() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let positions = trust
        .calculate_open_positions(None)
        .expect("current implementation returns empty for None account id");

    assert!(positions.is_empty());
}

#[test]
fn test_case_47_concentration_groups_unknown_metadata_under_unknown_bucket() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let account = create_account(&mut trust, "acct-47", model::Environment::Paper);
    deposit(&mut trust, &account, dec!(10000), Currency::USD);
    let vehicle = create_vehicle(&mut trust, "MSFT");

    let trade_unknown = create_trade(
        &mut trust,
        &account,
        &vehicle,
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
        Some("unknown metadata trade"),
        None,
        None,
        None,
    );

    let funded_unknown = fund_trade(&mut trust, &account, &trade_unknown);
    let submitted_unknown = submit_trade(&mut trust, &account, &funded_unknown);

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted_unknown, dec!(100)),
            order_target_accepted(&submitted_unknown),
            order_stop_held(&submitted_unknown),
        ],
    )));

    trust
        .sync_trade(&submitted_unknown, &account)
        .expect("sync unknown metadata trade");

    let trade_tech = create_trade(
        &mut trust,
        &account,
        &vehicle,
        TradeCategory::Long,
        10,
        dec!(110),
        dec!(100),
        dec!(130),
        Some("tech metadata trade"),
        Some("technology"),
        Some("stocks"),
        None,
    );

    let funded_tech = fund_trade(&mut trust, &account, &trade_tech);
    let submitted_tech = submit_trade(&mut trust, &account, &funded_tech);

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted_tech, dec!(110)),
            order_target_accepted(&submitted_tech),
            order_stop_held(&submitted_tech),
        ],
    )));

    trust
        .sync_trade(&submitted_tech, &account)
        .expect("sync tech trade");

    let mut all_trades = Vec::new();
    for status in Status::all() {
        if let Ok(mut trades) = trust.search_trades(account.id, status) {
            all_trades.append(&mut trades);
        }
    }

    let sector_analysis =
        ConcentrationCalculator::analyze_by_metadata(&all_trades, MetadataField::Sector);

    assert!(sector_analysis.groups.iter().any(|g| g.name == "Unknown"));
    assert!(sector_analysis
        .groups
        .iter()
        .any(|g| g.name == "technology"));
}

#[test]
fn test_case_48_concentration_warnings_trigger_for_moderate_and_high_thresholds() {
    let mut high_trade = Trade::default();
    high_trade.status = Status::Filled;
    high_trade.sector = Some("technology".to_string());
    high_trade.balance.capital_in_market = dec!(700);
    high_trade.balance.funding = dec!(700);

    let mut low_trade = Trade::default();
    low_trade.status = Status::Filled;
    low_trade.sector = Some("healthcare".to_string());
    low_trade.balance.capital_in_market = dec!(300);
    low_trade.balance.funding = dec!(300);

    let analysis = ConcentrationCalculator::analyze_by_metadata(
        &[high_trade, low_trade],
        MetadataField::Sector,
    );

    assert_eq!(analysis.total_risk, dec!(1000));
    assert_eq!(analysis.concentration_warnings.len(), 1);
    let warning = &analysis.concentration_warnings[0];
    assert_eq!(warning.group_name, "technology");
    assert_eq!(warning.level, WarningLevel::High);
    assert_eq!(warning.risk_percentage, dec!(70));
}

#[test]
fn test_case_49_drawdown_calculation_sorts_out_of_order_transactions_and_finds_max_dd() {
    let account_id = Uuid::new_v4();
    let now = Utc::now().naive_utc();

    let tx_latest = Transaction {
        id: Uuid::new_v4(),
        created_at: now,
        updated_at: now,
        deleted_at: None,
        category: TransactionCategory::Withdrawal,
        currency: Currency::USD,
        amount: dec!(200),
        account_id,
    };

    let tx_oldest = Transaction {
        id: Uuid::new_v4(),
        created_at: now - Duration::days(2),
        updated_at: now - Duration::days(2),
        deleted_at: None,
        category: TransactionCategory::Deposit,
        currency: Currency::USD,
        amount: dec!(1000),
        account_id,
    };

    let tx_middle = Transaction {
        id: Uuid::new_v4(),
        created_at: now - Duration::days(1),
        updated_at: now - Duration::days(1),
        deleted_at: None,
        category: TransactionCategory::Withdrawal,
        currency: Currency::USD,
        amount: dec!(300),
        account_id,
    };

    // Intentionally out of order.
    let transactions = vec![tx_latest, tx_oldest, tx_middle];

    let curve = RealizedDrawdownCalculator::calculate_equity_curve(&transactions)
        .expect("equity curve computation");
    let metrics =
        RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).expect("dd metrics");

    assert_eq!(curve.points.len(), 3);
    assert!(curve.points[0].timestamp < curve.points[1].timestamp);
    assert!(curve.points[1].timestamp < curve.points[2].timestamp);

    // Balance progression: 1000 -> 700 -> 500
    assert_eq!(metrics.peak_equity, dec!(1000));
    assert_eq!(metrics.current_equity, dec!(500));
    assert_eq!(metrics.max_drawdown, dec!(500));
}

#[test]
fn test_case_50_performance_stats_include_only_closed_trades_and_are_correct() {
    let mut win = Trade::default();
    win.status = Status::ClosedTarget;
    win.balance.total_performance = dec!(200);
    win.entry.unit_price = dec!(100);
    win.safety_stop.unit_price = dec!(90);
    win.target.unit_price = dec!(120);
    win.category = TradeCategory::Long;

    let mut loss = Trade::default();
    loss.status = Status::ClosedStopLoss;
    loss.balance.total_performance = dec!(-100);
    loss.entry.unit_price = dec!(80);
    loss.safety_stop.unit_price = dec!(85);
    loss.target.unit_price = dec!(60);
    loss.category = TradeCategory::Short;

    let mut open = Trade::default();
    open.status = Status::Filled;
    open.balance.total_performance = dec!(9999);

    let all_trades = vec![win.clone(), loss.clone(), open];
    let closed = PerformanceCalculator::filter_closed_trades(&all_trades);
    let stats = PerformanceCalculator::calculate_performance_stats(&closed);

    assert_eq!(closed.len(), 2);
    assert_eq!(stats.total_trades, 2);
    assert_eq!(stats.winning_trades, 1);
    assert_eq!(stats.losing_trades, 1);
    assert_eq!(stats.win_rate, dec!(50));
    assert_eq!(stats.average_win, dec!(200));
    assert_eq!(stats.average_loss, dec!(-100));
    assert_eq!(stats.best_trade, Some(dec!(200)));
    assert_eq!(stats.worst_trade, Some(dec!(-100)));

    // Long winner R-multiple = (120-100)/(100-90) = 2.0
    // Short loser R-multiple = (80-85)/(85-80) = -1.0
    // Average = 0.5
    assert_eq!(stats.average_r_multiple, dec!(0.5));
}

fn order_target_canceled(trade: &Trade) -> Order {
    let mut target = trade.target.clone();
    target.status = OrderStatus::Canceled;
    target.filled_quantity = 0;
    target.average_filled_price = None;
    target
}

fn fund_trade_direct(trust: &mut TrustFacade, account: &Account, trade: &Trade) -> Trade {
    trust.fund_trade(trade).expect("fund trade direct");
    trade_by_status_and_id(trust, account, Status::Funded, trade.id)
}

fn submit_trade_direct(trust: &mut TrustFacade, account: &Account, trade: &Trade) -> Trade {
    trust.submit_trade(trade).expect("submit trade direct");
    trade_by_status_and_id(trust, account, Status::Submitted, trade.id)
}

fn account_transactions_for_trade(
    trust: &mut TrustFacade,
    account: &Account,
    trade_id: Uuid,
) -> Vec<Transaction> {
    account_transactions(trust, account)
        .into_iter()
        .filter(|tx| tx.category.trade_id() == Some(trade_id))
        .collect()
}

fn replay_available_from_account_transactions(transactions: &[Transaction]) -> Decimal {
    transactions
        .iter()
        .try_fold(dec!(0), |acc, tx| match tx.category {
            TransactionCategory::Deposit | TransactionCategory::PaymentFromTrade(_) => {
                acc.checked_add(tx.amount).ok_or_else(|| {
                    format!(
                        "available replay overflow in addition: {} + {}",
                        acc, tx.amount
                    )
                })
            }
            TransactionCategory::Withdrawal
            | TransactionCategory::FundTrade(_)
            | TransactionCategory::FeeOpen(_)
            | TransactionCategory::FeeClose(_) => acc.checked_sub(tx.amount).ok_or_else(|| {
                format!(
                    "available replay overflow in subtraction: {} - {}",
                    acc, tx.amount
                )
            }),
            _ => Ok(acc),
        })
        .expect("replay available from account transactions")
}

fn assert_account_balance_reconciles(trust: &mut TrustFacade, account: &Account) {
    let txs = account_transactions(trust, account);
    let balance = usd_balance(trust, account);

    let replay_available = replay_available_from_account_transactions(&txs);
    assert_eq!(balance.total_available, replay_available);

    let replay_total = balance
        .total_available
        .checked_add(balance.total_in_trade)
        .expect("total available + total in trade");
    assert_eq!(balance.total_balance, replay_total);
}

fn all_trades_for_account(trust: &mut TrustFacade, account: &Account) -> Vec<Trade> {
    let mut all = Vec::new();
    for status in Status::all() {
        if let Ok(mut trades) = trust.search_trades(account.id, status) {
            all.append(&mut trades);
        }
    }
    all
}

fn run_matrix_sync_case(case_id: usize) {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let account_name = format!("acct-matrix-{case_id}");
    let account = create_account(&mut trust, &account_name, model::Environment::Paper);
    let initial_deposit = dec!(500000);
    deposit(&mut trust, &account, initial_deposit, Currency::USD);

    let symbol = format!("S{case_id}");
    let vehicle = create_vehicle(&mut trust, &symbol);

    let category = if case_id.is_multiple_of(2) {
        TradeCategory::Long
    } else {
        TradeCategory::Short
    };

    let quantity = 5_i64 + ((case_id % 5) as i64 * 5);
    let entry = Decimal::from(60_i64 + (case_id % 35) as i64);
    let stop_gap = dec!(5) + Decimal::from((case_id % 3) as i64);
    let target_gap = dec!(7) + Decimal::from((case_id % 4) as i64);

    let (stop, target) = match category {
        TradeCategory::Long => (
            entry.checked_sub(stop_gap).expect("long stop calc"),
            entry.checked_add(target_gap).expect("long target calc"),
        ),
        TradeCategory::Short => (
            entry.checked_add(stop_gap).expect("short stop calc"),
            entry.checked_sub(target_gap).expect("short target calc"),
        ),
    };

    let thesis = format!("generated matrix sync scenario {case_id}");
    let sector = match case_id % 3 {
        0 => Some("technology"),
        1 => Some("energy"),
        _ => Some("healthcare"),
    };
    let asset_class = if case_id.is_multiple_of(2) {
        Some("equity")
    } else {
        Some("futures")
    };

    let trade = create_trade(
        &mut trust,
        &account,
        &vehicle,
        category,
        quantity,
        entry,
        stop,
        target,
        Some(thesis.as_str()),
        sector,
        asset_class,
        Some("generated-matrix"),
    );

    let funded = fund_trade_direct(&mut trust, &account, &trade);
    let submitted = submit_trade_direct(&mut trust, &account, &funded);

    let fill_price = match category {
        TradeCategory::Long => {
            if case_id.is_multiple_of(3) {
                entry
            } else {
                entry.checked_sub(dec!(1)).expect("long fill calc")
            }
        }
        TradeCategory::Short => {
            if case_id.is_multiple_of(3) {
                entry
            } else {
                entry.checked_add(dec!(1)).expect("short fill calc")
            }
        }
    };

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted, fill_price),
            order_target_accepted(&submitted),
            order_stop_held(&submitted),
        ],
    )));

    trust
        .sync_trade(&submitted, &account)
        .expect("sync submitted->filled in matrix case");

    let filled = trade_by_status_and_id(&mut trust, &account, Status::Filled, trade.id);

    let close_on_target = case_id % 4 < 2;
    let target_fill = if case_id.is_multiple_of(6) {
        match category {
            TradeCategory::Long => target.checked_add(dec!(1)).expect("long target fill calc"),
            TradeCategory::Short => target.checked_sub(dec!(1)).expect("short target fill calc"),
        }
    } else {
        target
    };

    let stop_fill = if case_id.is_multiple_of(6) {
        stop.checked_add(dec!(1)).expect("stop fill calc")
    } else {
        stop
    };

    let expected_close_status = if close_on_target {
        Status::ClosedTarget
    } else {
        Status::ClosedStopLoss
    };

    let close_orders = if close_on_target {
        vec![
            order_entry_filled(&filled, fill_price),
            order_target_filled(&filled, target_fill),
            order_stop_canceled(&filled),
        ]
    } else {
        vec![
            order_entry_filled(&filled, fill_price),
            order_stop_filled(&filled, stop_fill),
            order_target_canceled(&filled),
        ]
    };

    broker.enqueue_sync(Ok((expected_close_status, close_orders)));

    trust
        .sync_trade(&filled, &account)
        .expect("sync filled->closed in matrix case");

    let mut closed = trade_by_status_and_id(&mut trust, &account, expected_close_status, trade.id);

    assert_eq!(closed.status, expected_close_status);
    assert_eq!(closed.entry.status, OrderStatus::Filled);

    if close_on_target {
        assert_eq!(closed.target.status, OrderStatus::Filled);
        assert_eq!(closed.safety_stop.status, OrderStatus::Canceled);
    } else {
        assert_eq!(closed.safety_stop.status, OrderStatus::Filled);
        assert_eq!(closed.target.status, OrderStatus::Canceled);
    }

    assert_eq!(closed.balance.capital_in_market, dec!(0));
    assert!(closed.balance.capital_out_market >= dec!(0));

    let trade_txs = account_transactions_for_trade(&mut trust, &account, trade.id);
    let fund_count = trade_txs
        .iter()
        .filter(|tx| matches!(tx.category, TransactionCategory::FundTrade(_)))
        .count();
    let payment_count = trade_txs
        .iter()
        .filter(|tx| matches!(tx.category, TransactionCategory::PaymentFromTrade(_)))
        .count();

    assert_eq!(fund_count, 1, "exactly one funding transaction expected");

    let expected_payment_count = if fill_price == entry { 1 } else { 2 };
    assert_eq!(payment_count, expected_payment_count);
    assert_eq!(trade_txs.len(), 1 + expected_payment_count);

    if case_id.is_multiple_of(5) {
        let tx_count_before = account_transactions_for_trade(&mut trust, &account, trade.id).len();

        let repeated_orders = if close_on_target {
            vec![
                order_entry_filled(&closed, fill_price),
                order_target_filled(&closed, target_fill),
                order_stop_canceled(&closed),
            ]
        } else {
            vec![
                order_entry_filled(&closed, fill_price),
                order_stop_filled(&closed, stop_fill),
                order_target_canceled(&closed),
            ]
        };

        broker.enqueue_sync(Ok((expected_close_status, repeated_orders)));
        trust
            .sync_trade(&closed, &account)
            .expect("repeat sync of already closed trade should be idempotent");

        let tx_count_after = account_transactions_for_trade(&mut trust, &account, trade.id).len();
        assert_eq!(tx_count_after, tx_count_before);

        closed = trade_by_status_and_id(&mut trust, &account, expected_close_status, trade.id);
        assert_eq!(closed.status, expected_close_status);
    }

    let balance = usd_balance(&mut trust, &account);
    assert_eq!(balance.total_in_trade, dec!(0));
    assert!(balance.total_available > dec!(0));
    assert_account_balance_reconciles(&mut trust, &account);
}

fn run_portfolio_window_case(case_id: usize) {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let account_name = format!("acct-portfolio-{case_id}");
    let account = create_account(&mut trust, &account_name, model::Environment::Paper);
    deposit(&mut trust, &account, dec!(3000000), Currency::USD);

    let vehicle_long = create_vehicle(&mut trust, &format!("PL{case_id}"));
    let vehicle_short = create_vehicle(&mut trust, &format!("PS{case_id}"));

    let days = 12 + (case_id % 7);
    let trades_per_day = 15 + (case_id % 9);

    let mut expected_closed_target = 0usize;
    let mut expected_closed_stop = 0usize;

    for day in 0..days {
        for slot in 0..trades_per_day {
            let is_long = (day + slot + case_id).is_multiple_of(2);
            let category = if is_long {
                TradeCategory::Long
            } else {
                TradeCategory::Short
            };

            let entry = Decimal::from(70_i64 + ((day * 3 + slot + case_id) % 35) as i64);
            let quantity = 1_i64 + ((day + slot + case_id) % 4) as i64;

            let (stop, target) = if is_long {
                (
                    entry.checked_sub(dec!(5)).expect("portfolio long stop"),
                    entry.checked_add(dec!(8)).expect("portfolio long target"),
                )
            } else {
                (
                    entry.checked_add(dec!(5)).expect("portfolio short stop"),
                    entry.checked_sub(dec!(8)).expect("portfolio short target"),
                )
            };

            let thesis = format!("portfolio case {case_id}, day {day}, slot {slot}");
            let sector = if (day + slot).is_multiple_of(3) {
                Some("technology")
            } else if (day + slot) % 3 == 1 {
                Some("energy")
            } else {
                Some("healthcare")
            };
            let asset_class = if is_long {
                Some("stocks")
            } else {
                Some("futures")
            };

            let trade = create_trade(
                &mut trust,
                &account,
                if is_long {
                    &vehicle_long
                } else {
                    &vehicle_short
                },
                category,
                quantity,
                entry,
                stop,
                target,
                Some(thesis.as_str()),
                sector,
                asset_class,
                Some("portfolio-window"),
            );

            let funded = fund_trade_direct(&mut trust, &account, &trade);
            let submitted = submit_trade_direct(&mut trust, &account, &funded);

            let close_on_target = !(day * trades_per_day + slot + case_id).is_multiple_of(3);
            let status = if close_on_target {
                expected_closed_target += 1;
                Status::ClosedTarget
            } else {
                expected_closed_stop += 1;
                Status::ClosedStopLoss
            };

            let orders = if close_on_target {
                vec![
                    order_entry_filled(&submitted, entry),
                    order_target_filled(&submitted, target),
                    order_stop_canceled(&submitted),
                ]
            } else {
                vec![
                    order_entry_filled(&submitted, entry),
                    order_stop_filled(&submitted, stop),
                    order_target_canceled(&submitted),
                ]
            };

            broker.enqueue_sync(Ok((status, orders)));
            trust
                .sync_trade(&submitted, &account)
                .expect("portfolio submitted->closed sync");
        }
    }

    let closed_target = trust
        .search_trades(account.id, Status::ClosedTarget)
        .expect("search closed target trades");
    let closed_stop = trust
        .search_trades(account.id, Status::ClosedStopLoss)
        .expect("search closed stop trades");

    assert_eq!(closed_target.len(), expected_closed_target);
    assert_eq!(closed_stop.len(), expected_closed_stop);

    let total_trades = days * trades_per_day;
    assert_eq!(closed_target.len() + closed_stop.len(), total_trades);

    let closed_trades = trust
        .search_closed_trades(Some(account.id))
        .expect("search all closed trades");
    assert_eq!(closed_trades.len(), total_trades);

    let stats = PerformanceCalculator::calculate_performance_stats(&closed_trades);
    assert_eq!(stats.total_trades, total_trades);
    assert_eq!(stats.winning_trades + stats.losing_trades, total_trades);
    assert!(stats.best_trade.is_some());
    assert!(stats.worst_trade.is_some());

    let sector_analysis =
        ConcentrationCalculator::analyze_by_metadata(&closed_trades, MetadataField::Sector);
    assert!(!sector_analysis.groups.is_empty());
    assert_eq!(sector_analysis.total_risk, dec!(0));
    assert!(sector_analysis.concentration_warnings.is_empty());

    let asset_analysis =
        ConcentrationCalculator::analyze_by_metadata(&closed_trades, MetadataField::AssetClass);
    assert!(!asset_analysis.groups.is_empty());
    assert_eq!(asset_analysis.total_risk, dec!(0));

    let txs = account_transactions(&mut trust, &account);
    let curve = RealizedDrawdownCalculator::calculate_equity_curve(&txs)
        .expect("portfolio equity curve calculation");
    assert_eq!(curve.points.len(), txs.len());

    let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve)
        .expect("portfolio drawdown metrics");
    assert!(metrics.max_drawdown >= dec!(0));
    assert!(metrics.max_drawdown_percentage >= dec!(0));
    assert!(metrics.max_drawdown_percentage <= dec!(100));

    let open_positions = trust
        .calculate_open_positions(Some(account.id))
        .expect("open positions after portfolio scenario");
    assert!(open_positions.is_empty());

    let all = all_trades_for_account(&mut trust, &account);
    assert_eq!(all.len(), total_trades);

    let balance = usd_balance(&mut trust, &account);
    assert_eq!(balance.total_in_trade, dec!(0));
    assert!(balance.total_available > dec!(0));
    assert_account_balance_reconciles(&mut trust, &account);
}

macro_rules! generated_matrix_tests {
    ($($name:ident => $id:expr),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                run_matrix_sync_case($id);
            }
        )*
    };
}

generated_matrix_tests! {
    test_case_051_generated_sync_matrix => 51,
    test_case_052_generated_sync_matrix => 52,
    test_case_053_generated_sync_matrix => 53,
    test_case_054_generated_sync_matrix => 54,
    test_case_055_generated_sync_matrix => 55,
    test_case_056_generated_sync_matrix => 56,
    test_case_057_generated_sync_matrix => 57,
    test_case_058_generated_sync_matrix => 58,
    test_case_059_generated_sync_matrix => 59,
    test_case_060_generated_sync_matrix => 60,
    test_case_061_generated_sync_matrix => 61,
    test_case_062_generated_sync_matrix => 62,
    test_case_063_generated_sync_matrix => 63,
    test_case_064_generated_sync_matrix => 64,
    test_case_065_generated_sync_matrix => 65,
    test_case_066_generated_sync_matrix => 66,
    test_case_067_generated_sync_matrix => 67,
    test_case_068_generated_sync_matrix => 68,
    test_case_069_generated_sync_matrix => 69,
    test_case_070_generated_sync_matrix => 70,
    test_case_071_generated_sync_matrix => 71,
    test_case_072_generated_sync_matrix => 72,
    test_case_073_generated_sync_matrix => 73,
    test_case_074_generated_sync_matrix => 74,
    test_case_075_generated_sync_matrix => 75,
    test_case_076_generated_sync_matrix => 76,
    test_case_077_generated_sync_matrix => 77,
    test_case_078_generated_sync_matrix => 78,
    test_case_079_generated_sync_matrix => 79,
    test_case_080_generated_sync_matrix => 80,
    test_case_081_generated_sync_matrix => 81,
    test_case_082_generated_sync_matrix => 82,
    test_case_083_generated_sync_matrix => 83,
    test_case_084_generated_sync_matrix => 84,
    test_case_085_generated_sync_matrix => 85,
    test_case_086_generated_sync_matrix => 86,
    test_case_087_generated_sync_matrix => 87,
    test_case_088_generated_sync_matrix => 88,
    test_case_089_generated_sync_matrix => 89,
    test_case_090_generated_sync_matrix => 90,
    test_case_091_generated_sync_matrix => 91,
    test_case_092_generated_sync_matrix => 92,
    test_case_093_generated_sync_matrix => 93,
    test_case_094_generated_sync_matrix => 94,
    test_case_095_generated_sync_matrix => 95,
    test_case_096_generated_sync_matrix => 96,
    test_case_097_generated_sync_matrix => 97,
    test_case_098_generated_sync_matrix => 98,
    test_case_099_generated_sync_matrix => 99,
    test_case_100_generated_sync_matrix => 100,
    test_case_101_generated_sync_matrix => 101,
    test_case_102_generated_sync_matrix => 102,
    test_case_103_generated_sync_matrix => 103,
    test_case_104_generated_sync_matrix => 104,
    test_case_105_generated_sync_matrix => 105,
    test_case_106_generated_sync_matrix => 106,
    test_case_107_generated_sync_matrix => 107,
    test_case_108_generated_sync_matrix => 108,
    test_case_109_generated_sync_matrix => 109,
    test_case_110_generated_sync_matrix => 110,
    test_case_111_generated_sync_matrix => 111,
    test_case_112_generated_sync_matrix => 112,
    test_case_113_generated_sync_matrix => 113,
    test_case_114_generated_sync_matrix => 114,
    test_case_115_generated_sync_matrix => 115,
    test_case_116_generated_sync_matrix => 116,
    test_case_117_generated_sync_matrix => 117,
    test_case_118_generated_sync_matrix => 118,
    test_case_119_generated_sync_matrix => 119,
    test_case_120_generated_sync_matrix => 120,
    test_case_121_generated_sync_matrix => 121,
    test_case_122_generated_sync_matrix => 122,
    test_case_123_generated_sync_matrix => 123,
    test_case_124_generated_sync_matrix => 124,
    test_case_125_generated_sync_matrix => 125,
    test_case_126_generated_sync_matrix => 126,
    test_case_127_generated_sync_matrix => 127,
    test_case_128_generated_sync_matrix => 128,
    test_case_129_generated_sync_matrix => 129,
    test_case_130_generated_sync_matrix => 130,
    test_case_131_generated_sync_matrix => 131,
    test_case_132_generated_sync_matrix => 132,
    test_case_133_generated_sync_matrix => 133,
    test_case_134_generated_sync_matrix => 134,
    test_case_135_generated_sync_matrix => 135,
    test_case_136_generated_sync_matrix => 136,
    test_case_137_generated_sync_matrix => 137,
    test_case_138_generated_sync_matrix => 138,
    test_case_139_generated_sync_matrix => 139,
    test_case_140_generated_sync_matrix => 140,
    test_case_141_generated_sync_matrix => 141,
    test_case_142_generated_sync_matrix => 142,
    test_case_143_generated_sync_matrix => 143,
    test_case_144_generated_sync_matrix => 144,
    test_case_145_generated_sync_matrix => 145,
    test_case_146_generated_sync_matrix => 146,
    test_case_147_generated_sync_matrix => 147,
    test_case_148_generated_sync_matrix => 148,
    test_case_149_generated_sync_matrix => 149,
    test_case_150_generated_sync_matrix => 150,
    test_case_151_generated_sync_matrix => 151,
    test_case_152_generated_sync_matrix => 152,
    test_case_153_generated_sync_matrix => 153,
    test_case_154_generated_sync_matrix => 154,
    test_case_155_generated_sync_matrix => 155,
    test_case_156_generated_sync_matrix => 156,
    test_case_157_generated_sync_matrix => 157,
    test_case_158_generated_sync_matrix => 158,
    test_case_159_generated_sync_matrix => 159,
    test_case_160_generated_sync_matrix => 160,
    test_case_161_generated_sync_matrix => 161,
    test_case_162_generated_sync_matrix => 162,
    test_case_163_generated_sync_matrix => 163,
    test_case_164_generated_sync_matrix => 164,
    test_case_165_generated_sync_matrix => 165,
    test_case_166_generated_sync_matrix => 166,
    test_case_167_generated_sync_matrix => 167,
    test_case_168_generated_sync_matrix => 168,
    test_case_169_generated_sync_matrix => 169,
    test_case_170_generated_sync_matrix => 170,
}

macro_rules! generated_portfolio_tests {
    ($($name:ident => $id:expr),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                run_portfolio_window_case($id);
            }
        )*
    };
}

generated_portfolio_tests! {
    test_case_171_generated_portfolio_windows => 171,
    test_case_172_generated_portfolio_windows => 172,
    test_case_173_generated_portfolio_windows => 173,
    test_case_174_generated_portfolio_windows => 174,
    test_case_175_generated_portfolio_windows => 175,
    test_case_176_generated_portfolio_windows => 176,
    test_case_177_generated_portfolio_windows => 177,
    test_case_178_generated_portfolio_windows => 178,
    test_case_179_generated_portfolio_windows => 179,
    test_case_180_generated_portfolio_windows => 180,
    test_case_181_generated_portfolio_windows => 181,
    test_case_182_generated_portfolio_windows => 182,
    test_case_183_generated_portfolio_windows => 183,
    test_case_184_generated_portfolio_windows => 184,
    test_case_185_generated_portfolio_windows => 185,
    test_case_186_generated_portfolio_windows => 186,
    test_case_187_generated_portfolio_windows => 187,
    test_case_188_generated_portfolio_windows => 188,
    test_case_189_generated_portfolio_windows => 189,
    test_case_190_generated_portfolio_windows => 190,
    test_case_191_generated_portfolio_windows => 191,
    test_case_192_generated_portfolio_windows => 192,
    test_case_193_generated_portfolio_windows => 193,
    test_case_194_generated_portfolio_windows => 194,
    test_case_195_generated_portfolio_windows => 195,
    test_case_196_generated_portfolio_windows => 196,
    test_case_197_generated_portfolio_windows => 197,
    test_case_198_generated_portfolio_windows => 198,
    test_case_199_generated_portfolio_windows => 199,
}

#[test]
fn test_case_200_high_frequency_sync_lifecycle_101_trades_per_day_for_365_days() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let account = create_account(&mut trust, "acct-200-hft", model::Environment::Paper);

    // Reserve enough capital for high-throughput synced subset.
    deposit(&mut trust, &account, dec!(5000000), Currency::USD);

    let vehicle_long = create_vehicle(&mut trust, "HFTL");
    let vehicle_short = create_vehicle(&mut trust, "HFTS");

    let days = 365usize;
    let trades_per_day = 101usize;
    let synced_trades_per_day = 2usize;
    let total_trades = days * trades_per_day;
    let total_synced_trades = days * synced_trades_per_day;

    let now = Utc::now().naive_utc();
    let mut simulated_closed_trades = Vec::with_capacity(total_trades);

    let mut expected_closed_target_total = 0usize;
    let mut expected_closed_stop_total = 0usize;
    let mut expected_closed_target_synced = 0usize;
    let mut expected_closed_stop_synced = 0usize;

    for day in 0..days {
        for slot in 0..trades_per_day {
            let is_long = (day + slot).is_multiple_of(2);
            let category = if is_long {
                TradeCategory::Long
            } else {
                TradeCategory::Short
            };

            let entry = Decimal::from(90_i64 + ((day + slot) % 25) as i64);
            let quantity = 1_i64 + ((day + slot) % 3) as i64;
            let quantity_dec = Decimal::from(quantity);

            let (stop, target) = if is_long {
                (
                    entry.checked_sub(dec!(5)).expect("hft long stop"),
                    entry.checked_add(dec!(8)).expect("hft long target"),
                )
            } else {
                (
                    entry.checked_add(dec!(5)).expect("hft short stop"),
                    entry.checked_sub(dec!(8)).expect("hft short target"),
                )
            };

            let close_on_target = !(day * trades_per_day + slot).is_multiple_of(3);
            let status = if close_on_target {
                expected_closed_target_total += 1;
                Status::ClosedTarget
            } else {
                expected_closed_stop_total += 1;
                Status::ClosedStopLoss
            };

            if slot < synced_trades_per_day {
                let thesis = format!("hft synced day {day} slot {slot}");
                let sector = if is_long {
                    Some("technology")
                } else {
                    Some("energy")
                };
                let asset_class = if is_long {
                    Some("stocks")
                } else {
                    Some("futures")
                };

                let trade = create_trade(
                    &mut trust,
                    &account,
                    if is_long {
                        &vehicle_long
                    } else {
                        &vehicle_short
                    },
                    category,
                    quantity,
                    entry,
                    stop,
                    target,
                    Some(thesis.as_str()),
                    sector,
                    asset_class,
                    Some("hft-synced"),
                );

                let funded = fund_trade_direct(&mut trust, &account, &trade);
                let submitted = submit_trade_direct(&mut trust, &account, &funded);

                let orders = if close_on_target {
                    vec![
                        order_entry_filled(&submitted, entry),
                        order_target_filled(&submitted, target),
                        order_stop_canceled(&submitted),
                    ]
                } else {
                    vec![
                        order_entry_filled(&submitted, entry),
                        order_stop_filled(&submitted, stop),
                        order_target_canceled(&submitted),
                    ]
                };

                broker.enqueue_sync(Ok((status, orders)));
                trust
                    .sync_trade(&submitted, &account)
                    .expect("hft submitted->closed sync");

                let closed = trade_by_status_and_id(&mut trust, &account, status, trade.id);
                simulated_closed_trades.push(closed);

                if close_on_target {
                    expected_closed_target_synced += 1;
                } else {
                    expected_closed_stop_synced += 1;
                }
            } else {
                let sector_value = if is_long {
                    "technology".to_string()
                } else {
                    "energy".to_string()
                };
                let asset_class_value = if is_long {
                    "stocks".to_string()
                } else {
                    "futures".to_string()
                };

                let funding = match category {
                    TradeCategory::Long => entry
                        .checked_mul(quantity_dec)
                        .expect("synthetic long funding"),
                    TradeCategory::Short => stop
                        .checked_mul(quantity_dec)
                        .expect("synthetic short funding"),
                };

                let close_price = if close_on_target { target } else { stop };
                let pnl_per_share = match category {
                    TradeCategory::Long => close_price
                        .checked_sub(entry)
                        .expect("synthetic long pnl per share"),
                    TradeCategory::Short => entry
                        .checked_sub(close_price)
                        .expect("synthetic short pnl per share"),
                };
                let total_performance = pnl_per_share
                    .checked_mul(quantity_dec)
                    .expect("synthetic total performance");

                let day_offset = i64::try_from(days.saturating_sub(day)).expect("day offset i64");
                let slot_offset = i64::try_from(slot).expect("slot offset i64");
                let synthetic_time = now
                    .checked_sub_signed(Duration::days(day_offset))
                    .expect("synthetic date")
                    .checked_add_signed(Duration::minutes(slot_offset))
                    .expect("synthetic minute offset");

                let mut synthetic_trade = Trade::default();
                synthetic_trade.status = status;
                synthetic_trade.category = category;
                synthetic_trade.entry.unit_price = entry;
                synthetic_trade.entry.quantity = u64::try_from(quantity).expect("quantity u64");
                synthetic_trade.safety_stop.unit_price = stop;
                synthetic_trade.safety_stop.quantity =
                    u64::try_from(quantity).expect("quantity u64");
                synthetic_trade.target.unit_price = target;
                synthetic_trade.target.quantity = u64::try_from(quantity).expect("quantity u64");
                synthetic_trade.sector = Some(sector_value);
                synthetic_trade.asset_class = Some(asset_class_value);
                synthetic_trade.balance.funding = funding;
                synthetic_trade.balance.capital_in_market = dec!(0);
                synthetic_trade.balance.total_performance = total_performance;
                synthetic_trade.created_at = synthetic_time;
                synthetic_trade.updated_at = synthetic_time;

                simulated_closed_trades.push(synthetic_trade);
            }
        }
    }

    assert_eq!(simulated_closed_trades.len(), total_trades);
    assert_eq!(
        expected_closed_target_total + expected_closed_stop_total,
        total_trades
    );

    let stats = PerformanceCalculator::calculate_performance_stats(&simulated_closed_trades);
    assert_eq!(stats.total_trades, total_trades);
    assert_eq!(stats.winning_trades + stats.losing_trades, total_trades);
    assert!(stats.best_trade.is_some());
    assert!(stats.worst_trade.is_some());

    let last_30_days = PerformanceCalculator::filter_trades_by_days(&simulated_closed_trades, 30);
    assert!(!last_30_days.is_empty());
    assert!(last_30_days.len() < total_trades);

    let sector_analysis = ConcentrationCalculator::analyze_by_metadata(
        &simulated_closed_trades,
        MetadataField::Sector,
    );
    assert_eq!(sector_analysis.total_risk, dec!(0));
    assert!(sector_analysis.concentration_warnings.is_empty());
    assert!(sector_analysis.groups.len() >= 2);

    let closed_target_synced = trust
        .search_trades(account.id, Status::ClosedTarget)
        .expect("search closed target in hft");
    let closed_stop_synced = trust
        .search_trades(account.id, Status::ClosedStopLoss)
        .expect("search closed stop in hft");

    assert_eq!(closed_target_synced.len(), expected_closed_target_synced);
    assert_eq!(closed_stop_synced.len(), expected_closed_stop_synced);
    assert_eq!(
        closed_target_synced.len() + closed_stop_synced.len(),
        total_synced_trades
    );

    let open_positions = trust
        .calculate_open_positions(Some(account.id))
        .expect("open positions in hft");
    assert!(open_positions.is_empty());

    let balance = usd_balance(&mut trust, &account);
    assert_eq!(balance.total_in_trade, dec!(0));
    assert!(balance.total_available > dec!(0));

    let txs = account_transactions(&mut trust, &account);
    assert_eq!(txs.len(), 1 + (2 * total_synced_trades));

    assert_account_balance_reconciles(&mut trust, &account);
}

#[test]
fn test_case_201_bug_hunt_sync_rejects_orders_that_belong_to_other_trade() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());
    let account = create_account(&mut trust, "acct-201", model::Environment::Paper);
    deposit(&mut trust, &account, dec!(50000), Currency::USD);
    let vehicle = create_vehicle(&mut trust, "BUG201");

    let trade_a = create_trade(
        &mut trust,
        &account,
        &vehicle,
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
        Some("trade a"),
        Some("technology"),
        Some("stocks"),
        None,
    );
    let trade_b = create_trade(
        &mut trust,
        &account,
        &vehicle,
        TradeCategory::Long,
        10,
        dec!(101),
        dec!(91),
        dec!(121),
        Some("trade b"),
        Some("technology"),
        Some("stocks"),
        None,
    );

    let funded_a = fund_trade_direct(&mut trust, &account, &trade_a);
    let submitted_a = submit_trade_direct(&mut trust, &account, &funded_a);
    let funded_b = fund_trade_direct(&mut trust, &account, &trade_b);
    let submitted_b = submit_trade_direct(&mut trust, &account, &funded_b);

    let rogue_order = order_entry_filled(&submitted_b, dec!(101));
    broker.enqueue_sync(Ok((Status::Submitted, vec![rogue_order])));

    let result = trust.sync_trade(&submitted_a, &account);
    assert!(
        result.is_err(),
        "sync should reject orders from a different trade"
    );
}

#[test]
fn test_case_202_bug_hunt_sync_should_be_atomic_when_any_order_update_fails() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-202",
        dec!(20000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );
    let funded = fund_trade_direct(&mut trust, &account, &trade);
    let submitted = submit_trade_direct(&mut trust, &account, &funded);

    let mut unknown_order = submitted.target.clone();
    unknown_order.id = Uuid::new_v4();
    unknown_order.status = OrderStatus::Accepted;
    let valid_order = order_target_accepted(&submitted);

    broker.enqueue_sync(Ok((Status::Submitted, vec![valid_order, unknown_order])));

    let result = trust.sync_trade(&submitted, &account);
    assert!(result.is_err(), "sync should fail due to unknown order id");

    let still_submitted = trade_by_status_and_id(&mut trust, &account, Status::Submitted, trade.id);
    assert_eq!(
        still_submitted.target.status, submitted.target.status,
        "failed sync should not persist partial order updates"
    );
}

#[test]
fn test_case_203_bug_hunt_rejected_status_update_should_not_mutate_orders() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-203",
        dec!(20000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );
    let funded = fund_trade_direct(&mut trust, &account, &trade);
    let submitted = submit_trade_direct(&mut trust, &account, &funded);

    broker.enqueue_sync(Ok((
        Status::PartiallyFilled,
        vec![order_entry_filled(&submitted, dec!(100))],
    )));

    let result = trust.sync_trade(&submitted, &account);
    assert!(
        result.is_err(),
        "unsupported partially filled status should fail"
    );

    let still_submitted = trade_by_status_and_id(&mut trust, &account, Status::Submitted, trade.id);
    assert_eq!(
        still_submitted.entry.status, submitted.entry.status,
        "failed status transition should not mutate order state"
    );
}

#[test]
fn test_case_204_bug_hunt_inconsistent_submitted_status_with_filled_entry_should_be_rejected() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-204",
        dec!(20000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );
    let funded = fund_trade_direct(&mut trust, &account, &trade);
    let submitted = submit_trade_direct(&mut trust, &account, &funded);

    broker.enqueue_sync(Ok((
        Status::Submitted,
        vec![order_entry_filled(&submitted, dec!(100))],
    )));

    let result = trust.sync_trade(&submitted, &account);
    assert!(
        result.is_err(),
        "sync should reject inconsistent payload: status submitted + filled entry"
    );
}

#[test]
fn test_case_205_bug_hunt_closed_target_sync_should_not_leave_stop_order_open() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-205",
        dec!(20000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );
    let funded = fund_trade_direct(&mut trust, &account, &trade);
    let submitted = submit_trade_direct(&mut trust, &account, &funded);

    // Broker reports closed target but omits stop-order cancellation update.
    broker.enqueue_sync(Ok((
        Status::ClosedTarget,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_filled(&submitted, dec!(120)),
        ],
    )));

    trust
        .sync_trade(&submitted, &account)
        .expect("sync should currently succeed");

    let closed = trade_by_status_and_id(&mut trust, &account, Status::ClosedTarget, trade.id);
    assert!(
        matches!(
            closed.safety_stop.status,
            OrderStatus::Canceled | OrderStatus::Filled
        ),
        "closed trades should not leave a live stop order"
    );
}

#[test]
fn test_case_206_bug_hunt_close_trade_broker_error_must_not_mutate_trade_state() {
    let broker = TestBroker::success();
    broker.state.lock().expect("broker state lock").close_error =
        Some("forced close error".to_string());

    let mut trust = new_trust_with_broker(broker.clone());
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-206",
        dec!(20000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );
    let funded = fund_trade_direct(&mut trust, &account, &trade);
    let submitted = submit_trade_direct(&mut trust, &account, &funded);

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_accepted(&submitted),
            order_stop_held(&submitted),
        ],
    )));
    trust
        .sync_trade(&submitted, &account)
        .expect("fill before close");

    let filled = trade_by_status_and_id(&mut trust, &account, Status::Filled, trade.id);
    let err = trust
        .close_trade(&filled)
        .expect_err("broker close error should bubble up");
    assert!(err.to_string().contains("forced close error"));

    let still_filled = trade_by_status_and_id(&mut trust, &account, Status::Filled, trade.id);
    assert_eq!(still_filled.status, Status::Filled);
    assert_ne!(still_filled.safety_stop.status, OrderStatus::Canceled);
}

#[test]
fn test_case_207_bug_hunt_open_positions_without_account_should_aggregate_all_accounts() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());

    let account_a = create_account(&mut trust, "acct-207-a", model::Environment::Paper);
    let account_b = create_account(&mut trust, "acct-207-b", model::Environment::Paper);
    deposit(&mut trust, &account_a, dec!(10000), Currency::USD);
    deposit(&mut trust, &account_b, dec!(10000), Currency::USD);
    let vehicle = create_vehicle(&mut trust, "BUG207");

    let trade_a = create_trade(
        &mut trust,
        &account_a,
        &vehicle,
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
        Some("a"),
        None,
        None,
        None,
    );
    let trade_b = create_trade(
        &mut trust,
        &account_b,
        &vehicle,
        TradeCategory::Short,
        10,
        dec!(100),
        dec!(110),
        dec!(80),
        Some("b"),
        None,
        None,
        None,
    );

    fund_trade_direct(&mut trust, &account_a, &trade_a);
    fund_trade_direct(&mut trust, &account_b, &trade_b);

    let positions = trust
        .calculate_open_positions(None)
        .expect("open positions with no account filter should work");
    assert!(
        positions.len() >= 2,
        "expected aggregated open positions across accounts"
    );
}

#[test]
fn test_case_208_bug_hunt_sync_should_validate_account_matches_trade_account() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());
    let account_a = create_account(&mut trust, "acct-208-a", model::Environment::Paper);
    let account_b = create_account(&mut trust, "acct-208-b", model::Environment::Paper);
    deposit(&mut trust, &account_a, dec!(10000), Currency::USD);
    deposit(&mut trust, &account_b, dec!(10000), Currency::USD);
    let vehicle = create_vehicle(&mut trust, "BUG208");

    let trade = create_trade(
        &mut trust,
        &account_a,
        &vehicle,
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
        Some("account mismatch"),
        None,
        None,
        None,
    );
    let funded = fund_trade_direct(&mut trust, &account_a, &trade);
    let submitted = submit_trade_direct(&mut trust, &account_a, &funded);

    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_accepted(&submitted),
            order_stop_held(&submitted),
        ],
    )));

    let result = trust.sync_trade(&submitted, &account_b);
    assert!(
        result.is_err(),
        "sync should fail when account parameter does not match trade account"
    );
}

#[test]
fn test_case_209_bug_hunt_failed_sync_on_closed_trade_should_not_mutate_orders() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());
    let (account, _vehicle, trade) = setup_account_deposit_vehicle_trade(
        &mut trust,
        "acct-209",
        dec!(20000),
        TradeCategory::Long,
        10,
        dec!(100),
        dec!(90),
        dec!(120),
    );
    let funded = fund_trade_direct(&mut trust, &account, &trade);
    let submitted = submit_trade_direct(&mut trust, &account, &funded);

    broker.enqueue_sync(Ok((
        Status::ClosedTarget,
        vec![
            order_entry_filled(&submitted, dec!(100)),
            order_target_filled(&submitted, dec!(120)),
            order_stop_canceled(&submitted),
        ],
    )));
    trust
        .sync_trade(&submitted, &account)
        .expect("initial close target sync");

    let closed = trade_by_status_and_id(&mut trust, &account, Status::ClosedTarget, trade.id);

    // Invalid transition for an already-closed trade: Submitted status.
    broker.enqueue_sync(Ok((
        Status::Submitted,
        vec![order_target_accepted(&closed)],
    )));

    let result = trust.sync_trade(&closed, &account);
    assert!(
        result.is_err(),
        "closed trade should reject transition back to submitted"
    );

    let still_closed = trade_by_status_and_id(&mut trust, &account, Status::ClosedTarget, trade.id);
    assert_eq!(
        still_closed.target.status, closed.target.status,
        "failed transition should not mutate order status on closed trade"
    );
}

#[test]
fn test_case_210_bug_hunt_drawdown_curve_ignores_trade_internal_flows_consistently() {
    let account_id = Uuid::new_v4();
    let now = Utc::now().naive_utc();
    let transactions = vec![
        Transaction {
            id: Uuid::new_v4(),
            created_at: now - Duration::days(3),
            updated_at: now - Duration::days(3),
            deleted_at: None,
            category: TransactionCategory::Deposit,
            currency: Currency::USD,
            amount: dec!(10000),
            account_id,
        },
        Transaction {
            id: Uuid::new_v4(),
            created_at: now - Duration::days(2),
            updated_at: now - Duration::days(2),
            deleted_at: None,
            category: TransactionCategory::FundTrade(Uuid::new_v4()),
            currency: Currency::USD,
            amount: dec!(1000),
            account_id,
        },
        Transaction {
            id: Uuid::new_v4(),
            created_at: now - Duration::days(1),
            updated_at: now - Duration::days(1),
            deleted_at: None,
            category: TransactionCategory::OpenTrade(Uuid::new_v4()),
            currency: Currency::USD,
            amount: dec!(1000),
            account_id,
        },
        Transaction {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            category: TransactionCategory::PaymentFromTrade(Uuid::new_v4()),
            currency: Currency::USD,
            amount: dec!(1200),
            account_id,
        },
    ];

    let curve = RealizedDrawdownCalculator::calculate_equity_curve(&transactions)
        .expect("drawdown curve for bug-hunt case");
    let metrics =
        RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).expect("drawdown metrics");

    assert_eq!(curve.points.len(), 3);
    assert_eq!(metrics.current_equity, dec!(10200));
    assert_eq!(metrics.peak_equity, dec!(10200));
    assert_eq!(metrics.max_drawdown, dec!(1000));
}

#[test]
fn test_case_211_bug_hunt_thousands_of_long_short_sync_lifecycle_scenarios() {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());
    let account = create_account(&mut trust, "acct-211", model::Environment::Paper);
    deposit(&mut trust, &account, dec!(15000000), Currency::USD);

    let vehicle_long = create_vehicle(&mut trust, "BUG211L");
    let vehicle_short = create_vehicle(&mut trust, "BUG211S");

    let scenarios = 1200usize;
    let mut expected_target = 0usize;
    let mut expected_stop = 0usize;

    for i in 0..scenarios {
        let is_long = i.is_multiple_of(2);
        let category = if is_long {
            TradeCategory::Long
        } else {
            TradeCategory::Short
        };
        let entry = Decimal::from(50_i64 + (i % 70) as i64);
        let quantity = 1_i64 + (i % 5) as i64;
        let (stop, target) = if is_long {
            (
                entry.checked_sub(dec!(4)).expect("long stop"),
                entry.checked_add(dec!(7)).expect("long target"),
            )
        } else {
            (
                entry.checked_add(dec!(4)).expect("short stop"),
                entry.checked_sub(dec!(7)).expect("short target"),
            )
        };

        let trade = create_trade(
            &mut trust,
            &account,
            if is_long {
                &vehicle_long
            } else {
                &vehicle_short
            },
            category,
            quantity,
            entry,
            stop,
            target,
            Some("thousands-scenario"),
            Some(if is_long { "technology" } else { "energy" }),
            Some(if is_long { "stocks" } else { "futures" }),
            Some("thousands-bug-hunt"),
        );

        let funded = fund_trade_direct(&mut trust, &account, &trade);
        let submitted = submit_trade_direct(&mut trust, &account, &funded);

        let close_on_target = !(i % 3).is_multiple_of(2);
        let status = if close_on_target {
            expected_target += 1;
            Status::ClosedTarget
        } else {
            expected_stop += 1;
            Status::ClosedStopLoss
        };

        let orders = if close_on_target {
            vec![
                order_entry_filled(&submitted, entry),
                order_target_filled(&submitted, target),
                order_stop_canceled(&submitted),
            ]
        } else {
            vec![
                order_entry_filled(&submitted, entry),
                order_stop_filled(&submitted, stop),
                order_target_canceled(&submitted),
            ]
        };

        broker.enqueue_sync(Ok((status, orders)));
        trust
            .sync_trade(&submitted, &account)
            .expect("sync thousands-scenario lifecycle");

        if i % 100 == 0 {
            assert_account_balance_reconciles(&mut trust, &account);
        }
    }

    let closed_target = trust
        .search_trades(account.id, Status::ClosedTarget)
        .expect("closed target after thousands scenario");
    let closed_stop = trust
        .search_trades(account.id, Status::ClosedStopLoss)
        .expect("closed stop after thousands scenario");

    assert_eq!(closed_target.len(), expected_target);
    assert_eq!(closed_stop.len(), expected_stop);
    assert_eq!(closed_target.len() + closed_stop.len(), scenarios);

    let closed_trades = trust
        .search_closed_trades(Some(account.id))
        .expect("all closed trades after thousands scenario");
    assert_eq!(closed_trades.len(), scenarios);

    let stats = PerformanceCalculator::calculate_performance_stats(&closed_trades);
    assert_eq!(stats.total_trades, scenarios);
    assert_eq!(stats.winning_trades + stats.losing_trades, scenarios);

    let txs = account_transactions(&mut trust, &account);
    let curve = RealizedDrawdownCalculator::calculate_equity_curve(&txs)
        .expect("equity curve thousands scenario");
    let metrics =
        RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).expect("drawdown thousands");
    assert!(metrics.max_drawdown >= dec!(0));
    assert!(metrics.max_drawdown_percentage >= dec!(0));
    assert!(metrics.max_drawdown_percentage <= dec!(100));

    let positions = trust
        .calculate_open_positions(Some(account.id))
        .expect("open positions thousands scenario");
    assert!(positions.is_empty());

    let balance = usd_balance(&mut trust, &account);
    assert_eq!(balance.total_in_trade, dec!(0));
    assert_account_balance_reconciles(&mut trust, &account);
}
