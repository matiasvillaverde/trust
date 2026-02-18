use chrono::Utc;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    Account, Broker, BrokerLog, Currency, DraftTrade, Environment, Order, OrderIds, RuleLevel,
    RuleName, Status, Trade, TradeCategory, TradingVehicleCategory, TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use uuid::Uuid;

struct RiskTestBroker;

impl Broker for RiskTestBroker {
    fn submit_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        Ok((
            BrokerLog::default(),
            OrderIds {
                entry: Uuid::new_v4(),
                target: Uuid::new_v4(),
                stop: Uuid::new_v4(),
            },
        ))
    }

    fn sync_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        let entry = Order {
            id: trade.entry.id,
            broker_order_id: trade.entry.broker_order_id,
            filled_quantity: trade.entry.quantity,
            average_filled_price: Some(trade.entry.unit_price),
            status: model::OrderStatus::Filled,
            filled_at: Some(Utc::now().naive_utc()),
            ..Default::default()
        };
        let target = Order {
            id: trade.target.id,
            broker_order_id: trade.target.broker_order_id,
            status: model::OrderStatus::Held,
            ..Default::default()
        };
        let stop = Order {
            id: trade.safety_stop.id,
            broker_order_id: trade.safety_stop.broker_order_id,
            status: model::OrderStatus::Held,
            ..Default::default()
        };
        Ok((
            Status::Filled,
            vec![entry, target, stop],
            BrokerLog::default(),
        ))
    }

    fn close_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        let mut target = trade.target.clone();
        target.status = model::OrderStatus::Filled;
        target.filled_quantity = target.quantity;
        target.average_filled_price = Some(trade.entry.unit_price);
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
    ) -> Result<Uuid, Box<dyn Error>> {
        Ok(Uuid::new_v4())
    }

    fn modify_target(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_target_price: Decimal,
    ) -> Result<Uuid, Box<dyn Error>> {
        Ok(Uuid::new_v4())
    }
}

fn create_trust() -> TrustFacade {
    let db = SqliteDatabase::new_in_memory();
    TrustFacade::new(Box::new(db), Box::new(RiskTestBroker))
}

fn setup_account_with_rules(
    trust: &mut TrustFacade,
    name: &str,
    capital: Decimal,
    risk_per_trade_pct: f32,
) -> Account {
    trust
        .create_account(name, "risk test", Environment::Paper, dec!(20), dec!(10))
        .expect("account creation should succeed");

    let account = trust.search_account(name).expect("account should exist");

    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            capital,
            &Currency::USD,
        )
        .expect("deposit should succeed");

    trust
        .create_rule(
            &account,
            &RuleName::RiskPerMonth(100.0),
            "month cap",
            &RuleLevel::Error,
        )
        .expect("risk per month rule should be created");

    trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(risk_per_trade_pct),
            "trade cap",
            &RuleLevel::Error,
        )
        .expect("risk per trade rule should be created");

    account
}

fn create_tv(trust: &mut TrustFacade, symbol: &str) -> model::TradingVehicle {
    trust
        .create_trading_vehicle(
            symbol,
            Some("US0000000000"),
            &TradingVehicleCategory::Stock,
            "NASDAQ",
        )
        .expect("trading vehicle creation should succeed")
}

fn create_trade(
    trust: &mut TrustFacade,
    account: &Account,
    category: TradeCategory,
    quantity: i64,
    stop: Decimal,
    entry: Decimal,
    target: Decimal,
) -> Trade {
    let tv = create_tv(
        trust,
        if category == TradeCategory::Long {
            "AAPL"
        } else {
            "TSLA"
        },
    );
    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity,
        currency: Currency::USD,
        category,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };

    trust
        .create_trade(draft, stop, entry, target)
        .expect("trade creation should succeed");

    trust
        .search_trades(account.id, Status::New)
        .expect("new trades should be queryable")
        .first()
        .expect("one new trade should exist")
        .clone()
}

fn create_filled_trade(trust: &mut TrustFacade, account: &Account) -> Trade {
    let new_trade = create_trade(
        trust,
        account,
        TradeCategory::Long,
        100,
        dec!(38),
        dec!(40),
        dec!(50),
    );
    trust.fund_trade(&new_trade).expect("fund should succeed");

    let funded = trust
        .search_trades(account.id, Status::Funded)
        .expect("funded trades query")
        .first()
        .expect("one funded trade")
        .clone();
    trust.submit_trade(&funded).expect("submit should succeed");

    let submitted = trust
        .search_trades(account.id, Status::Submitted)
        .expect("submitted trades query")
        .first()
        .expect("one submitted trade")
        .clone();

    trust
        .sync_trade(&submitted, account)
        .expect("sync should mark trade filled");

    trust
        .search_trades(account.id, Status::Filled)
        .expect("filled trades query")
        .first()
        .expect("one filled trade")
        .clone()
}

#[test]
fn short_trade_should_respect_risk_per_trade_limits() {
    let mut trust = create_trust();
    let account = setup_account_with_rules(&mut trust, "short-risk", dec!(100000), 1.0);

    // Short risk = (stop - entry) * qty = (150 - 100) * 600 = 30,000
    // Max allowed at 1% of 100,000 is 1,000 -> should be rejected.
    let short_trade = create_trade(
        &mut trust,
        &account,
        TradeCategory::Short,
        600,
        dec!(150),
        dec!(100),
        dec!(80),
    );

    let fund_result = trust.fund_trade(&short_trade);
    assert!(
        fund_result.is_err(),
        "short funding should fail when actual risk exceeds risk-per-trade limit"
    );
}

#[test]
fn short_trade_with_invalid_stop_entry_geometry_should_fail_with_clear_risk_message() {
    let mut trust = create_trust();
    let account = setup_account_with_rules(&mut trust, "short-geometry", dec!(100000), 2.0);

    // For short trades, stop must be above entry. This setup is inverted.
    let short_trade = create_trade(
        &mut trust,
        &account,
        TradeCategory::Short,
        50,
        dec!(90),
        dec!(100),
        dec!(80),
    );

    let error = trust
        .fund_trade(&short_trade)
        .expect_err("funding should fail for invalid short stop/entry geometry");
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("invalid risk setup") && message.contains("short"),
        "error message should clearly mention invalid short risk setup, got: {message}"
    );
}

#[test]
fn trade_with_negative_quantity_must_be_rejected() {
    let mut trust = create_trust();
    let account = setup_account_with_rules(&mut trust, "neg-qty", dec!(50000), 2.0);
    let tv = create_tv(&mut trust, "NVDA");

    let draft = DraftTrade {
        account,
        trading_vehicle: tv,
        quantity: -100,
        currency: Currency::USD,
        category: TradeCategory::Long,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };

    let create_result = trust.create_trade(draft, dec!(38), dec!(40), dec!(50));
    assert!(
        create_result.is_err(),
        "negative quantity should be rejected at trade creation"
    );
}

#[test]
fn protected_mode_should_not_block_trade_execution_actions() {
    let mut trust = create_trust();
    let account = setup_account_with_rules(&mut trust, "pm-trading", dec!(50000), 2.0);
    let filled = create_filled_trade(&mut trust, &account);

    trust.enable_protected_mode();
    let stop_result = trust.modify_stop(&filled, &account, dec!(39));
    assert!(
        stop_result.is_ok(),
        "protected mode should not block normal stop management"
    );

    let target_result = trust.modify_target(&filled, &account, dec!(60));
    assert!(
        target_result.is_ok(),
        "protected mode should not block normal target management"
    );

    let close_result = trust.close_trade(&filled);
    assert!(
        close_result.is_ok(),
        "protected mode should not block manual trade close"
    );
}

#[test]
fn protected_mode_should_block_risk_profile_changes_without_authorization() {
    let mut trust = create_trust();
    let account = setup_account_with_rules(&mut trust, "pm-risk-profile", dec!(50000), 2.0);

    trust.enable_protected_mode();

    let level_change = trust.change_level(
        account.id,
        4,
        "raise risk",
        model::LevelTrigger::ManualOverride,
    );
    assert!(
        level_change.is_err(),
        "changing account level should require protected authorization"
    );

    let new_rule = trust.create_rule(
        &account,
        &RuleName::RiskPerTrade(5.0),
        "raise per-trade risk",
        &RuleLevel::Error,
    );
    assert!(
        new_rule.is_err(),
        "changing risk rules should require protected authorization"
    );

    let rules_result =
        trust.set_level_adjustment_rules(account.id, &model::LevelAdjustmentRules::default());
    assert!(
        rules_result.is_err(),
        "changing level-adjustment rules should require protected authorization"
    );

    let advisory_result = trust.configure_advisory_thresholds(
        account.id,
        core::services::advisory::AdvisoryThresholds::default(),
    );
    assert!(
        advisory_result.is_err(),
        "changing advisory thresholds should require protected authorization"
    );

    let dist_config_result = trust.configure_distribution(
        account.id,
        dec!(0.40),
        dec!(0.30),
        dec!(0.30),
        dec!(100),
        "securepass123",
    );
    assert!(
        dist_config_result.is_err(),
        "changing distribution configuration should require protected authorization"
    );

    let dist_exec_result = trust.execute_distribution(account.id, dec!(1000), Currency::USD);
    assert!(
        dist_exec_result.is_err(),
        "executing distribution should require protected authorization"
    );
}
