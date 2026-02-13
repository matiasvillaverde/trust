use core::services::leveling::LevelPerformanceSnapshot;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::Broker;
use model::{Account, BrokerLog, Environment, Order, OrderIds, Status, Trade};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use uuid::Uuid;

fn create_trust() -> TrustFacade {
    let db = SqliteDatabase::new_in_memory();
    TrustFacade::new(Box::new(db), Box::new(MockBroker))
}

fn create_account(trust: &mut TrustFacade, name: &str) -> Uuid {
    trust
        .create_account(
            name,
            "level stress test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account")
        .id
}

fn evaluate_apply(trust: &mut TrustFacade, account_id: Uuid, snapshot: LevelPerformanceSnapshot) {
    trust
        .evaluate_level_transition(account_id, snapshot, true)
        .expect("evaluate and apply level transition");
}

fn snapshot(
    profitable_trades: u32,
    win_rate_percentage: Decimal,
    monthly_loss_percentage: Decimal,
    largest_loss_percentage: Decimal,
    consecutive_wins: u32,
) -> LevelPerformanceSnapshot {
    LevelPerformanceSnapshot {
        profitable_trades,
        win_rate_percentage,
        monthly_loss_percentage,
        largest_loss_percentage,
        consecutive_wins,
    }
}

#[test]
fn test_levels_happy_path_progressively_reach_level_four() {
    let mut trust = create_trust();
    let account_id = create_account(&mut trust, "level-happy");

    evaluate_apply(
        &mut trust,
        account_id,
        snapshot(15, dec!(75), dec!(-1), dec!(-0.4), 4),
    );
    evaluate_apply(
        &mut trust,
        account_id,
        snapshot(18, dec!(78), dec!(-0.8), dec!(-0.3), 5),
    );

    let level = trust.level_for_account(account_id).expect("level");
    assert_eq!(level.current_level, 4);
}

#[test]
fn test_levels_upper_bound_remains_capped_after_repeated_upgrades() {
    let mut trust = create_trust();
    let account_id = create_account(&mut trust, "level-cap");

    for _ in 0..10 {
        evaluate_apply(
            &mut trust,
            account_id,
            snapshot(25, dec!(90), dec!(-0.2), dec!(-0.1), 9),
        );
        evaluate_apply(
            &mut trust,
            account_id,
            snapshot(7, dec!(72), dec!(-0.6), dec!(-0.3), 3),
        );
    }

    let level = trust.level_for_account(account_id).expect("level");
    assert!(level.current_level <= 4);
}

#[test]
fn test_levels_no_change_when_thresholds_are_not_met() {
    let mut trust = create_trust();
    let account_id = create_account(&mut trust, "level-no-change");

    evaluate_apply(
        &mut trust,
        account_id,
        snapshot(3, dec!(55), dec!(-2), dec!(-1), 1),
    );

    let level = trust.level_for_account(account_id).expect("level");
    assert_eq!(level.current_level, 3);
    let history = trust
        .level_history_for_account(account_id, None)
        .expect("history");
    assert!(history.is_empty());
}

#[test]
fn test_levels_monthly_loss_breach_downgrades_one_level() {
    let mut trust = create_trust();
    let account_id = create_account(&mut trust, "level-monthly-breach");

    evaluate_apply(
        &mut trust,
        account_id,
        snapshot(2, dec!(40), dec!(-6), dec!(-1.2), 0),
    );

    let level = trust.level_for_account(account_id).expect("level");
    assert_eq!(level.current_level, 2);
}

#[test]
fn test_levels_largest_loss_breach_downgrades_one_level() {
    let mut trust = create_trust();
    let account_id = create_account(&mut trust, "level-largest-breach");

    evaluate_apply(
        &mut trust,
        account_id,
        snapshot(4, dec!(55), dec!(-3), dec!(-2.5), 1),
    );

    let level = trust.level_for_account(account_id).expect("level");
    assert_eq!(level.current_level, 2);
}

#[test]
fn test_levels_exceptional_performance_enters_cooldown() {
    let mut trust = create_trust();
    let account_id = create_account(&mut trust, "level-cooldown-enter");

    evaluate_apply(
        &mut trust,
        account_id,
        snapshot(22, dec!(88), dec!(-0.3), dec!(-0.2), 9),
    );

    let level = trust.level_for_account(account_id).expect("level");
    assert_eq!(level.current_level, 2);
    assert_eq!(level.status.to_string(), "cooldown");
}

#[test]
fn test_levels_failing_then_recovering_returns_to_normal_level() {
    let mut trust = create_trust();
    let account_id = create_account(&mut trust, "level-fail-then-recover");

    evaluate_apply(
        &mut trust,
        account_id,
        snapshot(1, dec!(25), dec!(-7), dec!(-3), 0),
    );
    evaluate_apply(
        &mut trust,
        account_id,
        snapshot(12, dec!(76), dec!(-0.7), dec!(-0.4), 4),
    );
    evaluate_apply(
        &mut trust,
        account_id,
        snapshot(7, dec!(72), dec!(-0.8), dec!(-0.4), 3),
    );

    let level = trust.level_for_account(account_id).expect("level");
    assert_eq!(level.current_level, 3);
    assert_eq!(level.status.to_string(), "normal");
}

#[test]
fn test_levels_recovery_does_not_overshoot_from_cooldown() {
    let mut trust = create_trust();
    let account_id = create_account(&mut trust, "level-recovery-bound");

    evaluate_apply(
        &mut trust,
        account_id,
        snapshot(24, dec!(92), dec!(-0.2), dec!(-0.1), 10),
    );
    evaluate_apply(
        &mut trust,
        account_id,
        snapshot(9, dec!(79), dec!(-0.4), dec!(-0.3), 4),
    );

    let level = trust.level_for_account(account_id).expect("level");
    assert_eq!(level.current_level, 3);
}

#[test]
fn test_levels_yearly_high_volume_simulation_stays_bounded() {
    let mut trust = create_trust();
    let account_id = create_account(&mut trust, "level-yearly-high-volume");

    // Simulate 365 trading days, each snapshot aggregating ~101 trades/day.
    for day in 0..365 {
        let phase = day % 30;
        let daily_snapshot = if phase < 10 {
            snapshot(72, dec!(78), dec!(-1.2), dec!(-0.7), 5)
        } else if phase < 15 {
            snapshot(84, dec!(91), dec!(-0.3), dec!(-0.2), 10)
        } else if phase < 20 {
            snapshot(28, dec!(42), dec!(-6.5), dec!(-2.7), 0)
        } else {
            snapshot(68, dec!(73), dec!(-1.5), dec!(-0.8), 3)
        };
        evaluate_apply(&mut trust, account_id, daily_snapshot);
    }

    let level = trust.level_for_account(account_id).expect("level");
    assert!(level.current_level <= 4);
    let history = trust
        .level_history_for_account(account_id, None)
        .expect("history");
    assert!(
        history.len() > 50,
        "expected frequent transitions in yearly high-volume simulation"
    );
}

#[test]
fn test_levels_long_run_with_mixed_paths_never_leaves_bounds() {
    let mut trust = create_trust();
    let account_id = create_account(&mut trust, "level-long-run-mixed");

    for i in 0..1_200 {
        let snapshot = match i % 6 {
            0 => snapshot(11, dec!(71), dec!(-1.0), dec!(-0.6), 3),
            1 => snapshot(21, dec!(89), dec!(-0.2), dec!(-0.2), 8),
            2 => snapshot(6, dec!(68), dec!(-0.9), dec!(-0.5), 2),
            3 => snapshot(3, dec!(35), dec!(-5.8), dec!(-1.4), 0),
            4 => snapshot(4, dec!(49), dec!(-3.0), dec!(-2.4), 1),
            _ => snapshot(8, dec!(66), dec!(-1.1), dec!(-0.9), 2),
        };
        evaluate_apply(&mut trust, account_id, snapshot);
    }

    let level = trust.level_for_account(account_id).expect("level");
    assert!(level.current_level <= 4);
    let history = trust
        .level_history_for_account(account_id, None)
        .expect("history");
    assert!(
        history.len() < 1_200,
        "not every evaluation should trigger a transition"
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
