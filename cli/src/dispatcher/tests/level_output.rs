#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use chrono::NaiveDate;
use chrono::TimeZone;
use chrono::Utc;
use core::services::leveling::LevelCriterionProgress;
use core::services::leveling::LevelDecision;
use core::services::leveling::LevelEvaluationOutcome;
use core::services::leveling::LevelPathProgress;
use core::services::leveling::LevelProgressReport;
use model::Level;
use model::LevelDirection;
use model::LevelStatus;
use model::LevelTrigger;
use rust_decimal_macros::dec;
use uuid::Uuid;

#[test]
fn test_level_payload_helpers_include_expected_fields() {
    let criterion = LevelCriterionProgress {
        key: "win_rate_percentage",
        comparator: ">=",
        actual: dec!(55),
        threshold: dec!(60),
        missing: dec!(5),
        met: false,
    };
    let path = LevelPathProgress {
        path: "performance_upgrade",
        trigger_type: LevelTrigger::PerformanceUpgrade,
        direction: LevelDirection::Upgrade,
        target_level: Some(4),
        criteria: vec![criterion.clone()],
        all_met: false,
    };
    let report = LevelProgressReport {
        current_level: 3,
        status: LevelStatus::Normal,
        upgrade_paths: vec![path.clone()],
        downgrade_paths: vec![],
    };

    let criterion_payload = crate::dispatcher::level_output::level_criterion_payload(&criterion);
    assert_eq!(criterion_payload["key"], "win_rate_percentage");
    assert_eq!(criterion_payload["actual"], "55");
    assert_eq!(criterion_payload["missing"], "5");

    let path_payload = crate::dispatcher::level_output::level_path_payload(&path);
    assert_eq!(path_payload["path"], "performance_upgrade");
    assert_eq!(path_payload["direction"], "Upgrade");
    assert_eq!(path_payload["target_level"], 4);

    let progress_payload = crate::dispatcher::level_output::level_progress_payload(&report);
    assert_eq!(progress_payload["current_level"], 3);
    assert_eq!(progress_payload["status"], "normal");
    assert_eq!(
        progress_payload["upgrade_paths"]
            .as_array()
            .expect("array")
            .len(),
        1
    );
}

#[test]
fn test_level_evaluation_payload_includes_decision_and_apply_state() {
    let now = Utc
        .with_ymd_and_hms(2026, 2, 25, 10, 0, 0)
        .unwrap()
        .naive_utc();
    let account_id = Uuid::new_v4();
    let current = Level {
        id: Uuid::new_v4(),
        created_at: now,
        updated_at: now,
        deleted_at: None,
        account_id,
        current_level: 3,
        risk_multiplier: dec!(1.0),
        status: LevelStatus::Normal,
        trades_at_level: 10,
        level_start_date: NaiveDate::from_ymd_opt(2026, 2, 1).expect("date"),
    };
    let applied = Level {
        current_level: 4,
        ..current.clone()
    };
    let outcome = LevelEvaluationOutcome {
        current_level: current,
        decision: Some(LevelDecision {
            target_level: 4,
            reason: "Strong performance".to_string(),
            trigger_type: LevelTrigger::PerformanceUpgrade,
            direction: LevelDirection::Upgrade,
        }),
        applied_level: Some(applied),
        progress: LevelProgressReport {
            current_level: 3,
            status: LevelStatus::Normal,
            upgrade_paths: vec![],
            downgrade_paths: vec![],
        },
    };

    let payload =
        crate::dispatcher::level_output::level_evaluation_payload(account_id, true, &outcome);
    assert_eq!(payload["report"], "level_evaluate");
    assert_eq!(payload["scope"]["account_id"], account_id.to_string());
    assert_eq!(payload["apply"], true);
    assert_eq!(payload["data"]["current_level"], 3);
    assert_eq!(payload["data"]["decision"]["target_level"], 4);
    assert_eq!(payload["data"]["decision"]["direction"], "Upgrade");
    assert_eq!(payload["data"]["applied_level"], 4);
}

#[test]
fn test_print_level_progress_text_handles_empty_and_non_empty_paths() {
    let empty_progress = LevelProgressReport {
        current_level: 1,
        status: LevelStatus::Normal,
        upgrade_paths: vec![],
        downgrade_paths: vec![],
    };
    crate::dispatcher::level_output::print_level_progress_text(&empty_progress);

    let path = LevelPathProgress {
        path: "policy",
        trigger_type: LevelTrigger::PerformanceUpgrade,
        direction: LevelDirection::Upgrade,
        target_level: Some(2),
        all_met: false,
        criteria: vec![LevelCriterionProgress {
            key: "win_rate",
            comparator: ">=",
            actual: dec!(55),
            threshold: dec!(60),
            missing: dec!(5),
            met: false,
        }],
    };
    let non_empty_progress = LevelProgressReport {
        current_level: 1,
        status: LevelStatus::Normal,
        upgrade_paths: vec![path.clone()],
        downgrade_paths: vec![path],
    };
    crate::dispatcher::level_output::print_level_progress_text(&non_empty_progress);
}

#[test]
fn test_print_level_path_text_handles_missing_target_level() {
    let path = LevelPathProgress {
        path: "upper-bound",
        trigger_type: LevelTrigger::PerformanceUpgrade,
        direction: LevelDirection::Upgrade,
        target_level: None,
        all_met: true,
        criteria: vec![],
    };

    crate::dispatcher::level_output::print_level_path_text("Upgrade", &path);
}
