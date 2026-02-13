use core::services::leveling::LevelPerformanceSnapshot;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::Broker;
use model::{Account, BrokerLog, Environment, LevelTrigger, Order, OrderIds, Status, Trade};
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
