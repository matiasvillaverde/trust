//! Legacy report signals command responsibility. Business mutations go through TrustFacade.
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use serde_json::Value;

pub(in crate::dispatcher) fn summarize_agent_status(breaches: &[String]) -> &'static str {
    if breaches.iter().any(|b| b.starts_with("critical:")) {
        "critical"
    } else if breaches.iter().any(|b| b.starts_with("warn:")) {
        "warn"
    } else {
        "ok"
    }
}

pub(in crate::dispatcher) fn summary_agent_signals(
    risk_pct: Decimal,
    top_concentration_pct: Decimal,
    max_drawdown_pct: Decimal,
    expectancy: Decimal,
) -> Value {
    let mut breaches: Vec<String> = Vec::new();
    let mut actions: Vec<String> = Vec::new();

    if risk_pct > dec!(20) {
        breaches.push("critical:capital_at_risk_above_20pct".to_string());
        actions.push("Reduce open exposure below 20% of equity".to_string());
    } else if risk_pct > dec!(10) {
        breaches.push("warn:capital_at_risk_above_10pct".to_string());
        actions.push("Review position sizing to reduce capital at risk".to_string());
    }

    if top_concentration_pct > dec!(60) {
        breaches.push("critical:concentration_above_60pct".to_string());
        actions.push("Diversify top concentration bucket below 60%".to_string());
    } else if top_concentration_pct > dec!(50) {
        breaches.push("warn:concentration_above_50pct".to_string());
        actions.push("Hedge or rebalance concentrated exposure".to_string());
    }

    if max_drawdown_pct > dec!(15) {
        breaches.push("critical:max_drawdown_above_15pct".to_string());
        actions.push("Pause new risk and review drawdown recovery plan".to_string());
    } else if max_drawdown_pct > dec!(10) {
        breaches.push("warn:max_drawdown_above_10pct".to_string());
        actions.push("Tighten risk controls during drawdown".to_string());
    }

    if expectancy < dec!(0) {
        breaches.push("warn:negative_expectancy".to_string());
        actions.push("Stop low-edge setups and review strategy quality".to_string());
    }

    json!({
        "status": summarize_agent_status(&breaches),
        "breaches": breaches,
        "recommended_actions": actions
    })
}

pub(in crate::dispatcher) fn metrics_agent_signals(
    expectancy: Decimal,
    win_rate_pct: Decimal,
    risk_of_ruin_proxy: Option<Decimal>,
    max_consecutive_losses: u32,
) -> Value {
    let mut breaches: Vec<String> = Vec::new();
    let mut actions: Vec<String> = Vec::new();

    if expectancy < dec!(0) {
        breaches.push("critical:negative_expectancy".to_string());
        actions.push("Disable strategy variants with negative expectancy".to_string());
    }
    if win_rate_pct < dec!(40) {
        breaches.push("warn:win_rate_below_40pct".to_string());
        actions.push("Increase selectivity and validate entries".to_string());
    }
    if max_consecutive_losses >= 5 {
        breaches.push("warn:loss_streak_ge_5".to_string());
        actions.push("Apply cooldown after long losing streaks".to_string());
    }
    if let Some(risk) = risk_of_ruin_proxy {
        if risk > dec!(0.4) {
            breaches.push("critical:risk_of_ruin_proxy_above_40pct".to_string());
            actions.push("Cut risk per trade and lower leverage".to_string());
        } else if risk > dec!(0.2) {
            breaches.push("warn:risk_of_ruin_proxy_above_20pct".to_string());
            actions.push("Reduce position size until risk-of-ruin improves".to_string());
        }
    }

    json!({
        "status": summarize_agent_status(&breaches),
        "breaches": breaches,
        "recommended_actions": actions
    })
}
