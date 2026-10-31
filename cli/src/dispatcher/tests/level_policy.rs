#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use crate::dispatcher::output::ReportOutputFormat;
use model::LevelAdjustmentRules;
use rust_decimal_macros::dec;

#[test]
fn test_apply_level_rule_update_validates_inputs_and_updates_fields() {
    let mut rules = LevelAdjustmentRules::default();
    crate::dispatcher::level_policy::apply_level_rule_update(
        &mut rules,
        "upgrade_win_rate_pct",
        "72.5",
        ReportOutputFormat::Json,
    )
    .expect("decimal update should work");
    assert_eq!(rules.upgrade_win_rate_pct, dec!(72.5));

    crate::dispatcher::level_policy::apply_level_rule_update(
        &mut rules,
        "upgrade_consecutive_wins",
        "4",
        ReportOutputFormat::Json,
    )
    .expect("u32 update should work");
    assert_eq!(rules.upgrade_consecutive_wins, 4);

    crate::dispatcher::level_policy::apply_level_rule_update(
        &mut rules,
        "min_trades_at_level_for_upgrade",
        "9",
        ReportOutputFormat::Json,
    )
    .expect("minimum trade count update should work");
    assert_eq!(rules.min_trades_at_level_for_upgrade, 9);

    let invalid_key = crate::dispatcher::level_policy::apply_level_rule_update(
        &mut rules,
        "not-a-real-key",
        "1",
        ReportOutputFormat::Json,
    )
    .expect_err("unknown key should fail");
    assert!(invalid_key.to_string().contains("invalid_rule_key"));

    let invalid_decimal = crate::dispatcher::level_policy::apply_level_rule_update(
        &mut rules,
        "upgrade_win_rate_pct",
        "abc",
        ReportOutputFormat::Json,
    )
    .expect_err("invalid decimal should fail");
    assert!(invalid_decimal.to_string().contains("invalid_rule_value"));

    let invalid_integer = crate::dispatcher::level_policy::apply_level_rule_update(
        &mut rules,
        "max_changes_in_30_days",
        "-1",
        ReportOutputFormat::Json,
    )
    .expect_err("invalid integer should fail");
    assert!(invalid_integer.to_string().contains("invalid_rule_value"));
}

#[test]
fn test_level_rules_payload_contains_all_keys() {
    let rules = LevelAdjustmentRules {
        monthly_loss_downgrade_pct: dec!(-6.0),
        single_loss_downgrade_pct: dec!(-2.5),
        upgrade_profitable_trades: 11,
        upgrade_win_rate_pct: dec!(71.0),
        upgrade_consecutive_wins: 4,
        cooldown_profitable_trades: 21,
        cooldown_win_rate_pct: dec!(86.5),
        cooldown_consecutive_wins: 7,
        recovery_profitable_trades: 6,
        recovery_win_rate_pct: dec!(66.0),
        recovery_consecutive_wins: 3,
        min_trades_at_level_for_upgrade: 8,
        max_changes_in_30_days: 5,
    };
    let payload = crate::dispatcher::level_policy::level_rules_payload(&rules);
    assert_eq!(payload["monthly_loss_downgrade_pct"], "-6");
    assert_eq!(payload["single_loss_downgrade_pct"], "-2.5");
    assert_eq!(payload["upgrade_profitable_trades"], 11);
    assert_eq!(payload["upgrade_win_rate_pct"], "71");
    assert_eq!(payload["upgrade_consecutive_wins"], 4);
    assert_eq!(payload["cooldown_profitable_trades"], 21);
    assert_eq!(payload["cooldown_win_rate_pct"], "86.5");
    assert_eq!(payload["cooldown_consecutive_wins"], 7);
    assert_eq!(payload["recovery_profitable_trades"], 6);
    assert_eq!(payload["recovery_win_rate_pct"], "66");
    assert_eq!(payload["recovery_consecutive_wins"], 3);
    assert_eq!(payload["min_trades_at_level_for_upgrade"], 8);
    assert_eq!(payload["max_changes_in_30_days"], 5);
}
