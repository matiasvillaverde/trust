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
    fees: Vec<model::FeeActivity>,
    sync_calls: usize,
    modify_stop_id: String,
    modify_target_id: String,
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
                fees: Vec::new(),
                sync_calls: 0,
                modify_stop_id: Uuid::from_u128(0x11111111111111111111111111111111).to_string(),
                modify_target_id: Uuid::from_u128(0x22222222222222222222222222222222).to_string(),
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

    fn modify_stop_id(&self) -> String {
        self.state
            .lock()
            .expect("broker state lock")
            .modify_stop_id
            .clone()
    }

    fn modify_target_id(&self) -> String {
        self.state
            .lock()
            .expect("broker state lock")
            .modify_target_id
            .clone()
    }
}

impl Broker for TestBroker {
    fn kind(&self) -> model::BrokerKind {
        model::BrokerKind::Alpaca
    }

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
            entry: format!("{}:entry", trade.id),
            target: format!("{}:target", trade.id),
            stop: format!("{}:stop", trade.id),
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
            state.sync_calls += 1;
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

    fn fetch_fee_activities(
        &self,
        _: &Trade,
        _: &Account,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<model::FeeActivity>, Box<dyn Error>> {
        Ok(self.state.lock().unwrap().fees.clone())
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
    ) -> Result<String, Box<dyn Error>> {
        let state = self.state.lock().expect("broker state lock");
        if let Some(message) = &state.modify_stop_error {
            return Err(message.clone().into());
        }
        Ok(state.modify_stop_id.clone())
    }

    fn modify_target(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_target_price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        let state = self.state.lock().expect("broker state lock");
        if let Some(message) = &state.modify_target_error {
            return Err(message.clone().into());
        }
        Ok(state.modify_target_id.clone())
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
        .create_trading_vehicle(
            symbol,
            Some(&isin),
            &TradingVehicleCategory::Stock,
            "Nasdaq",
        )
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
        quantity: quantity.into(),
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
    target.filled_quantity = 0.into();
    target.average_filled_price = None;
    target
}

fn order_stop_held(trade: &Trade) -> Order {
    let mut stop = trade.safety_stop.clone();
    stop.status = OrderStatus::Held;
    stop.filled_quantity = 0.into();
    stop.average_filled_price = None;
    stop
}

fn order_stop_canceled(trade: &Trade) -> Order {
    let mut stop = trade.safety_stop.clone();
    stop.status = OrderStatus::Canceled;
    stop.filled_quantity = 0.into();
    stop.average_filled_price = None;
    stop
}

#[allow(clippy::too_many_arguments)]
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

    assert_eq!(quantity, dec!(500));
}

#[test]
fn test_case_16_maximum_quantity_without_rules_uses_available_div_entry() {
    let mut trust = new_trust_with_broker(TestBroker::success());
    let account = create_account(&mut trust, "acct-16", model::Environment::Paper);
    deposit(&mut trust, &account, dec!(1000), Currency::USD);

    let quantity = trust
        .calculate_maximum_quantity(account.id, dec!(40), dec!(38), &Currency::USD)
        .expect("maximum quantity no rules");

    assert_eq!(quantity, dec!(25));
}

#[test]
fn test_case_17_duplicate_trading_vehicle_isin_is_rejected() {
    let mut trust = new_trust_with_broker(TestBroker::success());

    trust
        .create_trading_vehicle(
            "AAPL",
            Some("US0378331005"),
            &TradingVehicleCategory::Stock,
            "Nasdaq",
        )
        .expect("first vehicle");

    let err = trust
        .create_trading_vehicle(
            "AAPL",
            Some("US0378331005"),
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
            Some("us0378331005"),
            &TradingVehicleCategory::Stock,
            "NaSdAq",
        )
        .expect("create vehicle");

    assert_eq!(tv.symbol, "AAPL");
    assert_eq!(tv.isin.as_deref(), Some("US0378331005"));
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
fn test_case_25_submit_broker_error_retains_reconciliation_intent() {
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

    let pending = trade_by_status_and_id(&mut trust, &account, Status::Submitted, trade.id);
    assert_eq!(pending.status, Status::Submitted);
    assert!(pending.entry.broker_order_id.is_none());
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
fn test_case_35_close_trade_on_filled_persists_pending_market_exit() {
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

    let pending = trade_by_status_and_id(&mut trust, &account, Status::Filled, trade.id);
    assert_eq!(pending.status, Status::Filled);
    assert_ne!(pending.safety_stop.status, OrderStatus::Canceled);
    assert_eq!(pending.target.category, model::OrderCategory::Market);
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
fn test_case_41_sync_pending_manual_exit_to_closed_target_reconciles_close() {
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
        .expect("request manual market exit");

    let pending = trade_by_status_and_id(&mut trust, &account, Status::Filled, trade.id);

    // Broker later reports close target fill for the replacement market target.
    broker.enqueue_sync(Ok((
        Status::ClosedTarget,
        vec![order_target_filled(&pending, dec!(110))],
    )));

    trust
        .sync_trade(&pending, &account)
        .expect("sync pending manual exit to closed-target reconciliation");

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
    let mut high_trade = Trade {
        status: Status::Filled,
        sector: Some("technology".to_string()),
        ..Trade::default()
    };
    high_trade.balance.capital_in_market = dec!(700);
    high_trade.balance.funding = dec!(700);

    let mut low_trade = Trade {
        status: Status::Filled,
        sector: Some("healthcare".to_string()),
        ..Trade::default()
    };
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
    let mut win = Trade {
        status: Status::ClosedTarget,
        ..Trade::default()
    };
    win.balance.total_performance = dec!(200);
    win.entry.unit_price = dec!(100);
    win.safety_stop.unit_price = dec!(90);
    win.target.unit_price = dec!(120);
    win.category = TradeCategory::Long;

    let mut loss = Trade {
        status: Status::ClosedStopLoss,
        ..Trade::default()
    };
    loss.balance.total_performance = dec!(-100);
    loss.entry.unit_price = dec!(80);
    loss.safety_stop.unit_price = dec!(85);
    loss.target.unit_price = dec!(60);
    loss.category = TradeCategory::Short;

    let mut open = Trade {
        status: Status::Filled,
        ..Trade::default()
    };
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
    target.filled_quantity = 0.into();
    target.average_filled_price = None;
    target
}

fn fund_trade_direct(trust: &mut TrustFacade, account: &Account, trade: &Trade) -> Trade {
    let (funded, _tx, _balance, _trade_balance) =
        trust.fund_trade(trade).expect("fund trade direct");
    assert_eq!(funded.status, Status::Funded);
    assert_eq!(funded.account_id, account.id);
    funded
}

fn submit_trade_direct(trust: &mut TrustFacade, account: &Account, trade: &Trade) -> Trade {
    let (submitted, _log) = trust.submit_trade(trade).expect("submit trade direct");
    assert_eq!(submitted.status, Status::Submitted);
    assert_eq!(submitted.account_id, account.id);
    submitted
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

    let mut invalid_entry = order_entry_filled(&submitted, dec!(100));
    invalid_entry.filled_quantity = submitted.entry.quantity + dec!(1);
    broker.enqueue_sync(Ok((Status::PartiallyFilled, vec![invalid_entry])));

    let result = trust.sync_trade(&submitted, &account);
    assert!(
        result.is_err(),
        "cumulative fills above the order quantity should fail"
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
fn test_case_206_bug_hunt_close_trade_broker_error_retains_close_intent() {
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
    assert_eq!(still_filled.target.category, model::OrderCategory::Market);
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

#[path = "integration_test_use_cases/financial_scenarios.rs"]
mod financial_scenarios;
#[path = "integration_test_use_cases/performance.rs"]
mod performance;
#[path = "integration_test_use_cases/portfolio.rs"]
mod portfolio;
#[path = "integration_test_use_cases/volume.rs"]
mod volume;
