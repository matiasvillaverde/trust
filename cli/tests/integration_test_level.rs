use core::services::leveling::LevelPerformanceSnapshot;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::Broker;
use model::{
    Account, BrokerLog, Currency, DraftTrade, Environment, LevelAdjustmentRules, LevelTrigger,
    Order, OrderIds, RuleLevel, RuleName, Status, Trade, TradeCategory, TradingVehicleCategory,
    TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use uuid::Uuid;

fn create_trust() -> TrustFacade {
    let db = SqliteDatabase::new_in_memory();
    TrustFacade::new(Box::new(db), Box::new(MockBroker))
}

fn create_protected_trust() -> TrustFacade {
    let mut trust = create_trust();
    trust.enable_protected_mode();
    trust
}

fn setup_account_with_capital_and_rules(
    trust: &mut TrustFacade,
    name: &str,
    capital: rust_decimal::Decimal,
) -> Account {
    let account = trust
        .create_account(name, "test", Environment::Paper, dec!(20), dec!(10))
        .expect("create account");
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
            "max monthly risk",
            &RuleLevel::Error,
        )
        .expect("risk per month rule");
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(2.0),
            "max trade risk",
            &RuleLevel::Error,
        )
        .expect("risk per trade rule");
    account
}

fn create_new_trade(
    trust: &mut TrustFacade,
    account: &Account,
    quantity: i64,
    entry_price: rust_decimal::Decimal,
    stop_price: rust_decimal::Decimal,
    target_price: rust_decimal::Decimal,
) -> Trade {
    let vehicle = trust
        .create_trading_vehicle(
            "TSLA",
            Some("US88160R1014"),
            &TradingVehicleCategory::Stock,
            "NASDAQ",
        )
        .expect("create vehicle");
    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: vehicle,
        quantity,
        currency: Currency::USD,
        category: TradeCategory::Long,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };
    trust
        .create_trade(draft, stop_price, entry_price, target_price)
        .expect("create trade");
    trust
        .search_trades(account.id, Status::New)
        .expect("search new trades")
        .first()
        .expect("one trade")
        .clone()
}

#[test]
fn test_account_creation_provisions_default_level() {
    let mut trust = create_trust();

    let account = trust
        .create_account("level-acct", "test", Environment::Paper, dec!(20), dec!(10))
        .expect("create account");

    let level = trust.level_for_account(account.id).expect("level status");

    assert_eq!(level.current_level, 3);
    assert_eq!(level.risk_multiplier, dec!(1.00));
    assert_eq!(level.trades_at_level, 0);
}

#[test]
fn test_manual_level_change_records_history_for_upgrade_and_downgrade() {
    let mut trust = create_trust();
    let account = trust
        .create_account(
            "level-change",
            "test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");

    let (upgraded, up_event) = trust
        .change_level(
            account.id,
            4,
            "Strong consistency",
            LevelTrigger::ManualReview,
        )
        .expect("upgrade");

    assert_eq!(upgraded.current_level, 4);
    assert_eq!(up_event.old_level, 3);
    assert_eq!(up_event.new_level, 4);

    let (downgraded, down_event) = trust
        .change_level(
            account.id,
            2,
            "Breach monthly loss",
            LevelTrigger::Custom("risk_control".to_string()),
        )
        .expect("downgrade");

    assert_eq!(downgraded.current_level, 2);
    assert_eq!(down_event.old_level, 4);
    assert_eq!(down_event.new_level, 2);

    let history = trust
        .level_history_for_account(account.id, None)
        .expect("level history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].id, down_event.id);
    assert_eq!(history[1].id, up_event.id);
}

#[test]
fn test_policy_evaluation_applies_upgrade_and_downgrade() {
    let mut trust = create_trust();
    let account = trust
        .create_account(
            "level-policy",
            "test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");

    let upgrade_snapshot = LevelPerformanceSnapshot {
        profitable_trades: 11,
        win_rate_percentage: dec!(72),
        monthly_loss_percentage: dec!(-1),
        largest_loss_percentage: dec!(-0.8),
        consecutive_wins: 5,
    };

    let upgrade = trust
        .evaluate_level_transition(account.id, upgrade_snapshot, true)
        .expect("evaluate apply upgrade");
    assert!(upgrade.decision.is_some());
    assert_eq!(
        upgrade
            .applied_level
            .expect("applied upgrade")
            .current_level,
        4
    );

    let downgrade_snapshot = LevelPerformanceSnapshot {
        profitable_trades: 20,
        win_rate_percentage: dec!(80),
        monthly_loss_percentage: dec!(-6),
        largest_loss_percentage: dec!(-3.2),
        consecutive_wins: 0,
    };

    let downgrade = trust
        .evaluate_level_transition(account.id, downgrade_snapshot, true)
        .expect("evaluate apply downgrade");

    assert!(downgrade.decision.is_some());
    assert_eq!(
        downgrade
            .applied_level
            .expect("applied downgrade")
            .current_level,
        3
    );

    let history = trust
        .level_history_for_account(account.id, Some(30))
        .expect("recent history");
    assert_eq!(history.len(), 2);
}

#[test]
fn test_policy_cooldown_then_quick_recovery() {
    let mut trust = create_trust();
    let account = trust
        .create_account(
            "level-cooldown",
            "test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");

    let exceptional = LevelPerformanceSnapshot {
        profitable_trades: 25,
        win_rate_percentage: dec!(90),
        monthly_loss_percentage: dec!(-0.5),
        largest_loss_percentage: dec!(-0.3),
        consecutive_wins: 10,
    };

    let cooldown = trust
        .evaluate_level_transition(account.id, exceptional, true)
        .expect("apply cooldown");
    let applied_cooldown = cooldown.applied_level.expect("cooldown level");
    assert_eq!(applied_cooldown.current_level, 2);
    assert_eq!(applied_cooldown.status.to_string(), "cooldown");

    let recover = LevelPerformanceSnapshot {
        profitable_trades: 6,
        win_rate_percentage: dec!(72),
        monthly_loss_percentage: dec!(-1.0),
        largest_loss_percentage: dec!(-0.6),
        consecutive_wins: 3,
    };

    let recovered = trust
        .evaluate_level_transition(account.id, recover, true)
        .expect("recover level");
    let applied_recovered = recovered.applied_level.expect("recovered level");
    assert_eq!(applied_recovered.current_level, 3);
    assert_eq!(applied_recovered.status.to_string(), "normal");
}

#[test]
fn test_manual_level_change_is_idempotent_for_same_retry() {
    let mut trust = create_trust();
    let account = trust
        .create_account(
            "level-idempotent",
            "test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");

    let (_, first_event) = trust
        .change_level(
            account.id,
            2,
            "Risk review downgrade",
            LevelTrigger::ManualReview,
        )
        .expect("first change");

    let (_, second_event) = trust
        .change_level(
            account.id,
            2,
            "Risk review downgrade",
            LevelTrigger::ManualReview,
        )
        .expect("idempotent retry");

    assert_eq!(first_event.id, second_event.id);

    let history = trust
        .level_history_for_account(account.id, Some(1))
        .expect("history");
    assert_eq!(history.len(), 1);
}

#[test]
fn test_core_protected_mode_blocks_level_mutation_without_authorization() {
    let mut trust = create_protected_trust();
    let account = trust
        .create_account(
            "level-core-protected",
            "test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect_err("account create should be blocked");
    assert!(
        account.to_string().contains("create_account"),
        "expected protected mutation error"
    );
}

#[test]
fn test_core_protected_mode_allows_single_authorized_level_mutation() {
    let mut trust = create_protected_trust();
    trust.authorize_protected_mutation();
    let account = trust
        .create_account(
            "level-core-authorized",
            "test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("authorized account create");

    trust.authorize_protected_mutation();
    let changed = trust.change_level(account.id, 2, "risk control", LevelTrigger::ManualReview);
    assert!(changed.is_ok(), "authorized level mutation should succeed");

    let second_without_auth =
        trust.change_level(account.id, 3, "revert", LevelTrigger::ManualOverride);
    assert!(
        second_without_auth.is_err(),
        "authorization should be one-shot"
    );
}

#[test]
fn test_level_adjusted_quantity_changes_with_level() {
    let mut trust = create_trust();
    let account = setup_account_with_capital_and_rules(&mut trust, "level-sized", dec!(50_000));

    let sized_l3 = trust
        .calculate_level_adjusted_quantity(account.id, dec!(40), dec!(38), &Currency::USD)
        .expect("size L3");
    assert_eq!(sized_l3.base_quantity, 500);
    assert_eq!(sized_l3.final_quantity, 500);
    assert_eq!(sized_l3.level_multiplier, dec!(1));

    trust
        .change_level(account.id, 2, "reduce size", LevelTrigger::ManualReview)
        .expect("change level");

    let sized_l2 = trust
        .calculate_level_adjusted_quantity(account.id, dec!(40), dec!(38), &Currency::USD)
        .expect("size L2");
    assert_eq!(sized_l2.base_quantity, 500);
    assert_eq!(sized_l2.final_quantity, 250);
    assert_eq!(sized_l2.level_multiplier, dec!(0.5));
}

#[test]
fn test_funding_rejects_quantity_above_level_adjusted_limit() {
    let mut trust = create_trust();
    let account =
        setup_account_with_capital_and_rules(&mut trust, "level-fund-guard", dec!(50_000));
    trust
        .change_level(account.id, 2, "recovery mode", LevelTrigger::ManualReview)
        .expect("change level");

    let trade = create_new_trade(&mut trust, &account, 300, dec!(40), dec!(38), dec!(50));
    let result = trust.fund_trade(&trade);
    assert!(result.is_err(), "funding should enforce level-adjusted cap");
    let error = result.expect_err("expected funding error");
    assert!(
        error.to_string().contains("level-adjusted maximum"),
        "expected level-adjusted guardrail in error"
    );
}

#[test]
fn test_level_adjustment_rules_update_requires_authorization_in_protected_mode() {
    let mut trust = create_protected_trust();
    trust.authorize_protected_mutation();
    let account = trust
        .create_account(
            "level-rules-protected",
            "test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("authorized account create");

    let rules = LevelAdjustmentRules::default();
    let without_auth = trust.set_level_adjustment_rules(account.id, &rules);
    assert!(
        without_auth.is_err(),
        "rules mutation should require explicit authorization"
    );

    trust.authorize_protected_mutation();
    let with_auth = trust.set_level_adjustment_rules(account.id, &rules);
    assert!(
        with_auth.is_ok(),
        "authorized rules mutation should succeed"
    );
}

#[test]
fn test_custom_level_rules_change_upgrade_behavior() {
    let mut trust = create_trust();
    let account = trust
        .create_account(
            "level-rules-behavior",
            "test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");

    let mut strict = trust
        .level_adjustment_rules_for_account(account.id)
        .expect("load default rules");
    strict.upgrade_profitable_trades = 20;
    strict.upgrade_win_rate_pct = dec!(90);
    strict.upgrade_consecutive_wins = 4;
    trust
        .set_level_adjustment_rules(account.id, &strict)
        .expect("persist strict rules");

    let borderline_snapshot = LevelPerformanceSnapshot {
        profitable_trades: 12,
        win_rate_percentage: dec!(75),
        monthly_loss_percentage: dec!(-1),
        largest_loss_percentage: dec!(-0.5),
        consecutive_wins: 4,
    };
    let no_upgrade = trust
        .evaluate_level_transition(account.id, borderline_snapshot, false)
        .expect("evaluate strict rules");
    assert!(
        no_upgrade.decision.is_none(),
        "strict rules should block default-like upgrade snapshot"
    );

    let strong_snapshot = LevelPerformanceSnapshot {
        profitable_trades: 23,
        win_rate_percentage: dec!(92),
        monthly_loss_percentage: dec!(-0.8),
        largest_loss_percentage: dec!(-0.4),
        consecutive_wins: 5,
    };
    let upgrade = trust
        .evaluate_level_transition(account.id, strong_snapshot, false)
        .expect("evaluate upgraded snapshot");
    assert_eq!(upgrade.decision.expect("decision").target_level, 4);
}

#[test]
fn test_invalid_level_adjustment_rules_are_rejected() {
    let mut trust = create_trust();
    let account = trust
        .create_account(
            "level-rules-invalid",
            "test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");

    let invalid = LevelAdjustmentRules {
        monthly_loss_downgrade_pct: dec!(1),
        ..LevelAdjustmentRules::default()
    };
    let result = trust.set_level_adjustment_rules(account.id, &invalid);
    assert!(result.is_err(), "invalid rules should be rejected");
}

struct MockBroker;

impl Broker for MockBroker {
    fn submit_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        unimplemented!()
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
        unimplemented!()
    }

    fn modify_stop(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_stop_price: Decimal,
    ) -> Result<Uuid, Box<dyn Error>> {
        unimplemented!()
    }

    fn modify_target(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_price: Decimal,
    ) -> Result<Uuid, Box<dyn Error>> {
        unimplemented!()
    }
}
