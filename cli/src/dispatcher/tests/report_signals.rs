#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use rust_decimal_macros::dec;

#[test]
fn test_summarize_agent_status_prioritizes_critical_then_warn() {
    assert_eq!(
        crate::dispatcher::report_signals::summarize_agent_status(&[]),
        "ok"
    );
    assert_eq!(
        crate::dispatcher::report_signals::summarize_agent_status(&["warn:thing".to_string()]),
        "warn"
    );
    assert_eq!(
        crate::dispatcher::report_signals::summarize_agent_status(&[
            "warn:thing".to_string(),
            "critical:other".to_string()
        ]),
        "critical"
    );
}

#[test]
fn test_summary_agent_signals_returns_expected_breaches_and_actions() {
    let payload = crate::dispatcher::report_signals::summary_agent_signals(
        dec!(21),
        dec!(61),
        dec!(16),
        dec!(-0.1),
    );
    assert_eq!(payload["status"], "critical");
    let breaches = payload["breaches"]
        .as_array()
        .expect("breaches should be array");
    assert!(breaches
        .iter()
        .any(|v| v.as_str() == Some("critical:capital_at_risk_above_20pct")));
    assert!(breaches
        .iter()
        .any(|v| v.as_str() == Some("critical:concentration_above_60pct")));
    assert!(breaches
        .iter()
        .any(|v| v.as_str() == Some("critical:max_drawdown_above_15pct")));
    assert!(breaches
        .iter()
        .any(|v| v.as_str() == Some("warn:negative_expectancy")));
    assert!(!payload["recommended_actions"]
        .as_array()
        .expect("actions should be array")
        .is_empty());
}

#[test]
fn test_metrics_agent_signals_handles_warn_and_critical_paths() {
    let payload = crate::dispatcher::report_signals::metrics_agent_signals(
        dec!(-1),
        dec!(39),
        Some(dec!(0.41)),
        5,
    );
    assert_eq!(payload["status"], "critical");
    let breaches = payload["breaches"]
        .as_array()
        .expect("breaches should be array");
    assert!(breaches
        .iter()
        .any(|v| v.as_str() == Some("critical:negative_expectancy")));
    assert!(breaches
        .iter()
        .any(|v| v.as_str() == Some("warn:win_rate_below_40pct")));
    assert!(breaches
        .iter()
        .any(|v| v.as_str() == Some("warn:loss_streak_ge_5")));
    assert!(breaches
        .iter()
        .any(|v| v.as_str() == Some("critical:risk_of_ruin_proxy_above_40pct")));

    let ok_payload =
        crate::dispatcher::report_signals::metrics_agent_signals(dec!(1), dec!(60), None, 1);
    assert_eq!(ok_payload["status"], "ok");
    assert_eq!(ok_payload["breaches"].as_array().expect("array").len(), 0);
}

#[test]
fn test_summary_agent_signals_warn_and_ok_paths() {
    let warn_payload = crate::dispatcher::report_signals::summary_agent_signals(
        dec!(11),
        dec!(55),
        dec!(11),
        dec!(0.5),
    );
    assert_eq!(warn_payload["status"], "warn");
    let warn_breaches = warn_payload["breaches"]
        .as_array()
        .expect("warn breaches array");
    assert!(warn_breaches
        .iter()
        .any(|entry| entry == "warn:capital_at_risk_above_10pct"));
    assert!(warn_breaches
        .iter()
        .any(|entry| entry == "warn:concentration_above_50pct"));
    assert!(warn_breaches
        .iter()
        .any(|entry| entry == "warn:max_drawdown_above_10pct"));

    let ok_payload = crate::dispatcher::report_signals::summary_agent_signals(
        dec!(5),
        dec!(20),
        dec!(3),
        dec!(1),
    );
    assert_eq!(ok_payload["status"], "ok");
    assert_eq!(
        ok_payload["breaches"]
            .as_array()
            .expect("ok breaches")
            .len(),
        0
    );
    assert_eq!(
        ok_payload["recommended_actions"]
            .as_array()
            .expect("ok actions")
            .len(),
        0
    );
}

#[test]
fn test_metrics_agent_signals_warn_only_and_ok_paths() {
    let warn_payload = crate::dispatcher::report_signals::metrics_agent_signals(
        dec!(1),
        dec!(35),
        Some(dec!(0.3)),
        5,
    );
    assert_eq!(warn_payload["status"], "warn");
    let warn_breaches = warn_payload["breaches"]
        .as_array()
        .expect("warn breaches array");
    assert!(warn_breaches
        .iter()
        .any(|entry| entry == "warn:win_rate_below_40pct"));
    assert!(warn_breaches
        .iter()
        .any(|entry| entry == "warn:loss_streak_ge_5"));
    assert!(warn_breaches
        .iter()
        .any(|entry| entry == "warn:risk_of_ruin_proxy_above_20pct"));
    assert!(!warn_breaches
        .iter()
        .any(|entry| entry == "critical:negative_expectancy"));

    let ok_payload = crate::dispatcher::report_signals::metrics_agent_signals(
        dec!(1.5),
        dec!(55),
        Some(dec!(0.1)),
        2,
    );
    assert_eq!(ok_payload["status"], "ok");
    assert_eq!(
        ok_payload["breaches"]
            .as_array()
            .expect("ok breaches")
            .len(),
        0
    );
}
