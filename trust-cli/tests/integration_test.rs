use chrono::Utc;
use rust_decimal_macros::dec;
use std::error::Error;
use trust_core::TrustFacade;
use trust_db_sqlite::SqliteDatabase;
use trust_model::{
    Account, BrokerLog, Currency, Order, OrderIds, RuleLevel, RuleName, Status, Trade,
    TradeCategory, TradingVehicleCategory, TransactionCategory,
};
use trust_model::{Broker, DraftTrade, OrderStatus};
use uuid::Uuid;

fn create_trust() -> TrustFacade {
    let db = SqliteDatabase::new_in_memory();
    TrustFacade::new(
        Box::new(db),
        Box::new(MockBroker::new(BrokerResponse::orders_accepted)),
    )
}

#[test]
fn test_account_creation() {
    let mut trust = create_trust();

    trust
        .create_account("alpaca", "default", trust_model::Environment::Paper)
        .unwrap();
    let account = trust.search_account("alpaca").unwrap();
    let accounts: Vec<Account> = trust.search_all_accounts().unwrap();

    assert_eq!(account.name, "alpaca");
    assert_eq!(account.description, "default");
    assert_eq!(account.environment, trust_model::Environment::Paper);
    assert_eq!(accounts.len(), 1);
}

#[test]
fn test_transactions() {
    let mut trust = create_trust();

    let account = trust
        .create_account("alpaca", "default", trust_model::Environment::Paper)
        .unwrap();

    let (tx, overview) = trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(40000),
            &Currency::USD,
        )
        .unwrap();

    assert_eq!(tx.amount, dec!(40000));
    assert_eq!(tx.category, TransactionCategory::Deposit);
    assert_eq!(tx.currency, Currency::USD);
    assert_eq!(tx.account_id, account.id);
    assert_eq!(overview.account_id, account.id);
    assert_eq!(overview.currency, Currency::USD);
    assert_eq!(overview.total_available, dec!(40000));
    assert_eq!(overview.total_balance, dec!(40000));
    assert_eq!(overview.total_in_trade, dec!(0));
    assert_eq!(overview.taxed, dec!(0));
}

#[test]
fn test_multiple_transactions() {
    let mut trust = create_trust();

    let account = trust
        .create_account("alpaca", "default", trust_model::Environment::Paper)
        .unwrap();

    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(40000),
            &Currency::USD,
        )
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(883.23),
            &Currency::USD,
        )
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Withdrawal,
            dec!(121.21),
            &Currency::USD,
        )
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(243.12),
            &Currency::USD,
        )
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Withdrawal,
            dec!(4992.0002),
            &Currency::USD,
        )
        .unwrap();

    let (tx, overview) = trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(2032.1),
            &Currency::USD,
        )
        .unwrap();

    assert_eq!(tx.amount, dec!(2032.1));
    assert_eq!(tx.category, TransactionCategory::Deposit);
    assert_eq!(tx.currency, Currency::USD);
    assert_eq!(tx.account_id, account.id);
    assert_eq!(overview.account_id, account.id);
    assert_eq!(overview.currency, Currency::USD);
    assert_eq!(overview.total_available, dec!(38045.2398));
    assert_eq!(overview.total_balance, dec!(38045.2398));
    assert_eq!(overview.total_in_trade, dec!(0));
    assert_eq!(overview.taxed, dec!(0));
}

#[test]
fn test_risk_rules() {
    let mut trust = create_trust();

    let account = trust
        .create_account("alpaca", "default", trust_model::Environment::Paper)
        .unwrap();

    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(50000),
            &Currency::USD,
        )
        .unwrap();

    trust
        .create_rule(
            &account,
            &RuleName::RiskPerMonth(6.0),
            "description",
            &RuleLevel::Error,
        )
        .unwrap();
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(2.0),
            "description",
            &RuleLevel::Error,
        )
        .unwrap();

    let quantity = trust
        .calculate_maximum_quantity(account.id, dec!(40), dec!(38), &Currency::USD)
        .unwrap();

    assert_eq!(quantity, 500);
}

fn create_trade(
    broker_response: fn(trade: &Trade) -> (Status, Vec<Order>),
) -> (TrustFacade, Account, Trade) {
    let db = SqliteDatabase::new_in_memory();
    let mut trust = TrustFacade::new(Box::new(db), Box::new(MockBroker::new(broker_response)));

    // 1. Create account and deposit money
    trust
        .create_account("alpaca", "default", trust_model::Environment::Paper)
        .expect("Failed to create account");
    let account = trust.search_account("alpaca").unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(50000),
            &Currency::USD,
        )
        .expect("Failed to deposit money");
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerMonth(6.0),
            "description",
            &RuleLevel::Error,
        )
        .expect("Failed to create rule risk per month");
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(2.0),
            "description",
            &RuleLevel::Error,
        )
        .expect("Failed to create rule risk per trade");

    // 2. Create trading vehicle
    let tv = trust
        .create_trading_vehicle(
            "TSLA",
            "US88160R1014",
            &TradingVehicleCategory::Stock,
            "NASDAQ",
        )
        .expect("Failed to create trading vehicle");

    // 3. Create trade
    let trade = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 500,
        currency: Currency::USD,
        category: TradeCategory::Long,
    };

    trust
        .create_trade(trade, dec!(38), dec!(40), dec!(50))
        .expect("Failed to create trade");
    let trade = trust
        .search_trades(account.id, Status::New)
        .expect("Failed to find trade")
        .first()
        .unwrap()
        .clone();

    // 4. Fund trade
    trust.fund_trade(&trade).unwrap();
    let trade = trust
        .search_trades(account.id, Status::Funded)
        .expect("Failed to find trade with status funded")
        .first()
        .unwrap()
        .clone();

    // 5. Submit trade to the Broker
    trust.submit_trade(&trade).unwrap();
    let trade = trust
        .search_trades(account.id, Status::Submitted)
        .expect("Failed to find trade with status submitted")
        .first()
        .unwrap()
        .clone();

    (trust, account, trade)
}

#[test]
fn test_trade_submit_entry_accepted() {
    let (trust, account, trade) = create_trade(BrokerResponse::orders_accepted);
    let mut trust = trust;

    // 6. Sync trade with the Broker - Entry is accepted
    trust
        .sync_trade(&trade, &account)
        .expect("Failed to sync trade with broker when entry is accepted");
    let trade = trust
        .search_trades(account.id, Status::Submitted)
        .expect("Failed to find trade with status submitted 2")
        .first()
        .unwrap()
        .clone();

    assert_eq!(trade.status, Status::Submitted);

    // Assert Entry
    assert_eq!(trade.entry.quantity, 500);
    assert_eq!(trade.entry.unit_price, dec!(40));
    assert_eq!(trade.entry.average_filled_price, None);
    assert_eq!(trade.entry.filled_quantity, 0);
    assert_eq!(trade.entry.status, OrderStatus::Accepted);

    // Assert Target
    assert_eq!(trade.target.quantity, 500);
    assert_eq!(trade.target.unit_price, dec!(50));
    assert_eq!(trade.target.average_filled_price, None);
    assert_eq!(trade.target.filled_quantity, 0);
    assert_eq!(trade.target.status, OrderStatus::Held);

    // Assert Stop
    assert_eq!(trade.safety_stop.quantity, 500);
    assert_eq!(trade.safety_stop.unit_price, dec!(38));
    assert_eq!(trade.target.average_filled_price, None);
    assert_eq!(trade.safety_stop.filled_quantity, 0);
    assert_eq!(trade.safety_stop.status, OrderStatus::Held);

    // Assert Account Overview
    let account = trust.search_account("alpaca").unwrap();
    let overview = trust.search_overview(account.id, &Currency::USD).unwrap();

    assert_eq!(overview.currency, Currency::USD);
    assert_eq!(overview.total_available, dec!(30000));
    assert_eq!(overview.total_balance, dec!(50000));
    assert_eq!(overview.total_in_trade, dec!(20000));
    assert_eq!(overview.taxed, dec!(0));
}

#[test]
fn test_trade_entry_filled() {
    let (trust, account, trade) = create_trade(BrokerResponse::orders_entry_filled);
    let mut trust = trust;

    // 7. Sync trade with the Broker - Entry is filled
    trust
        .sync_trade(&trade, &account)
        .expect("Failed to sync trade with broker when entry is filled");
    let trade = trust
        .search_trades(account.id, Status::Filled)
        .expect("Failed to find trade with status submitted 2")
        .first()
        .unwrap()
        .clone();

    // Assert Status
    assert_eq!(trade.status, Status::Filled);

    // Assert Entry
    assert_eq!(trade.entry.quantity, 500);
    assert_eq!(trade.entry.unit_price, dec!(40));
    assert_eq!(trade.entry.average_filled_price, Some(dec!(39.9)));
    assert_eq!(trade.entry.filled_quantity, 500);
    assert_eq!(trade.entry.status, OrderStatus::Filled);

    // Assert Target
    assert_eq!(trade.target.quantity, 500);
    assert_eq!(trade.target.unit_price, dec!(50));
    assert_eq!(trade.target.average_filled_price, None);
    assert_eq!(trade.target.filled_quantity, 0);
    assert_eq!(trade.target.status, OrderStatus::Accepted);

    // Assert Stop
    assert_eq!(trade.safety_stop.quantity, 500);
    assert_eq!(trade.safety_stop.unit_price, dec!(38));
    assert_eq!(trade.target.average_filled_price, None);
    assert_eq!(trade.safety_stop.filled_quantity, 0);
    assert_eq!(trade.safety_stop.status, OrderStatus::Held);

    // TODO: The average filled price is less than the unit price, so the remaining money that was
    // not used to buy the shares should be returned to the account.
}

#[test]
fn test_trade_target_filled() {
    let (trust, account, trade) = create_trade(BrokerResponse::orders_target_filled);
    let mut trust = trust;

    // 9. Sync trade with the Broker - Target is filled
    let (status, orders) = trust.sync_trade(&trade, &account).unwrap();

    println!("{:?}", status);
    println!("{:?}", orders);

    let trade = trust
        .search_trades(account.id, Status::ClosedTarget)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    assert_eq!(trade.status, Status::ClosedTarget);

    // Assert Entry
    assert_eq!(trade.entry.quantity, 500);
    assert_eq!(trade.entry.unit_price, dec!(40));
    assert_eq!(trade.entry.average_filled_price, Some(dec!(39.9)));
    assert_eq!(trade.entry.filled_quantity, 500);
    assert_eq!(trade.entry.status, OrderStatus::Filled);

    // Assert Target
    assert_eq!(trade.target.quantity, 500);
    assert_eq!(trade.target.unit_price, dec!(50));
    assert_eq!(trade.target.average_filled_price, Some(dec!(52.9)));
    assert_eq!(trade.target.filled_quantity, 500);
    assert_eq!(trade.target.status, OrderStatus::Filled);

    // Assert Stop
    assert_eq!(trade.safety_stop.quantity, 500);
    assert_eq!(trade.safety_stop.unit_price, dec!(38));
    assert_eq!(trade.safety_stop.average_filled_price, None);
    assert_eq!(trade.safety_stop.filled_quantity, 0);
    assert_eq!(trade.safety_stop.status, OrderStatus::Canceled);

    // Assert Account Overview
    let account = trust.search_account("alpaca").unwrap();
    let overview = trust.search_overview(account.id, &Currency::USD).unwrap();
    assert_eq!(overview.currency, Currency::USD);
    assert_eq!(overview.total_available, dec!(56450)); // TODO Calculate the exact amount
    assert_eq!(overview.total_balance, dec!(56450)); // TODO Calculate the exact amount
    assert_eq!(overview.total_in_trade, dec!(0));
    assert_eq!(overview.taxed, dec!(0));
}

// TODO: Add for stop filled

struct BrokerResponse;

impl BrokerResponse {
    fn orders_accepted(trade: &Trade) -> (Status, Vec<Order>) {
        let entry = Order {
            id: trade.entry.id,
            broker_order_id: Some(Uuid::parse_str("b6b12dc0-8e21-4d2e-8315-907d3116a6b8").unwrap()),
            filled_quantity: 0,
            average_filled_price: None,
            status: OrderStatus::Accepted,
            filled_at: None,
            expired_at: None,
            cancelled_at: None,
            ..Default::default()
        };

        let target = Order {
            id: trade.target.id,
            broker_order_id: Some(Uuid::parse_str("b6b12dc0-8e21-4d2e-8315-907d3116a6b8").unwrap()),
            filled_quantity: 0,
            average_filled_price: None,
            status: OrderStatus::Held,
            filled_at: None,
            expired_at: None,
            cancelled_at: None,
            ..Default::default()
        };

        let stop = Order {
            id: trade.safety_stop.id,
            broker_order_id: Some(Uuid::parse_str("b6b12dc0-8e21-4d2e-8315-907d3116a6b8").unwrap()),
            filled_quantity: 0,
            average_filled_price: None,
            status: OrderStatus::Held,
            filled_at: None,
            expired_at: None,
            cancelled_at: None,
            ..Default::default()
        };

        (Status::Submitted, vec![entry, target, stop])
    }

    fn orders_entry_filled(trade: &Trade) -> (Status, Vec<Order>) {
        let entry = Order {
            id: trade.entry.id,
            broker_order_id: Some(Uuid::parse_str("b6b12dc0-8e21-4d2e-8315-907d3116a6b8").unwrap()),
            filled_quantity: 500,
            average_filled_price: Some(dec!(39.9)),
            status: OrderStatus::Filled,
            filled_at: Some(Utc::now().naive_utc()),
            expired_at: None,
            cancelled_at: None,
            ..Default::default()
        };

        let target = Order {
            id: trade.target.id,
            broker_order_id: Some(Uuid::parse_str("b6b12dc0-8e21-4d2e-8315-907d3116a6b8").unwrap()),
            filled_quantity: 0,
            average_filled_price: None,
            status: OrderStatus::Accepted,
            filled_at: None,
            expired_at: None,
            cancelled_at: None,
            ..Default::default()
        };

        let stop = Order {
            id: trade.safety_stop.id,
            broker_order_id: Some(Uuid::parse_str("b6b12dc0-8e21-4d2e-8315-907d3116a6b8").unwrap()),
            filled_quantity: 0,
            average_filled_price: None,
            status: OrderStatus::Held,
            filled_at: None,
            expired_at: None,
            cancelled_at: None,
            ..Default::default()
        };

        (Status::Filled, vec![entry, target, stop])
    }

    fn orders_target_filled(trade: &Trade) -> (Status, Vec<Order>) {
        let entry = Order {
            id: trade.entry.id,
            broker_order_id: Some(Uuid::parse_str("b6b12dc0-8e21-4d2e-8315-907d3116a6b8").unwrap()),
            filled_quantity: 500,
            average_filled_price: Some(dec!(39.9)),
            status: OrderStatus::Filled,
            filled_at: Some(Utc::now().naive_utc()),
            expired_at: None,
            cancelled_at: None,
            ..Default::default()
        };

        let target = Order {
            id: trade.target.id,
            broker_order_id: Some(Uuid::parse_str("b6b12dc0-8e21-4d2e-8315-907d3116a6b8").unwrap()),
            filled_quantity: 500,
            average_filled_price: Some(dec!(52.9)),
            status: OrderStatus::Filled,
            filled_at: Some(Utc::now().naive_utc()),
            expired_at: None,
            cancelled_at: None,
            ..Default::default()
        };

        let stop = Order {
            id: trade.safety_stop.id,
            broker_order_id: Some(Uuid::parse_str("b6b12dc0-8e21-4d2e-8315-907d3116a6b8").unwrap()),
            filled_quantity: 0,
            average_filled_price: None,
            status: OrderStatus::Canceled,
            filled_at: None,
            expired_at: None,
            cancelled_at: None,
            ..Default::default()
        };

        (Status::ClosedTarget, vec![entry, target, stop])
    }
}

struct MockBroker {
    sync_trade: fn(trade: &Trade) -> (Status, Vec<Order>),
}

impl MockBroker {
    fn new(provider: fn(trade: &Trade) -> (Status, Vec<Order>)) -> MockBroker {
        MockBroker {
            sync_trade: provider,
        }
    }
}

impl Broker for MockBroker {
    fn submit_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        let log = BrokerLog::default();
        let ids = OrderIds {
            entry: Uuid::parse_str("b6b12dc0-8e21-4d2e-8315-907d3116a6b8").unwrap(),
            target: Uuid::parse_str("90e41b1e-9089-444d-9f68-c204a4d32914").unwrap(),
            stop: Uuid::parse_str("8654f70e-3b42-4014-a9ac-5a7101989aad").unwrap(),
        };
        Ok((log, ids))
    }

    fn sync_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>), Box<dyn Error>> {
        Ok((self.sync_trade)(trade))
    }
}
