//! Deliberately permissive paper fixtures; these are not deployment defaults.
#![allow(dead_code)]
use chrono::{Duration, Utc};
use model::{
    account_safety::BrokerAccountScope,
    portfolio_risk::{BrokerMargin, RiskMarketData},
    MarketBar,
};
use rust_decimal::Decimal;
use serde_json::{json, Value};
pub fn controls() -> Value {
    json!({
        "max_portfolio_heat_pct":"10", "max_trade_heat_pct":"5", "max_stress_loss_pct":"20",
        "min_margin_cushion_pct":"5", "maintenance_margin_stress_pct":"50",
        "max_instrument_pct":"100", "max_sector_pct":"100", "max_cluster_pct":"100",
        "max_quote_age_seconds":30, "max_history_age_seconds":604800, "volatility_lookback_bars":2,
        "atr_multiplier":"1", "max_spread_bps":"100", "max_entry_deviation_bps":"500",
        "max_volume_participation_pct":"10", "max_book_participation_pct":"100",
        "slippage_bps":"0", "fee_per_unit":"0", "minimum_fee":"0",
        "drawdown_steps":[{"drawdown_pct":"0","risk_multiplier":"1"},{"drawdown_pct":"5","risk_multiplier":"0.5"},{"drawdown_pct":"10","risk_multiplier":"0"}],
        "recovery_delay_seconds":3600, "recovery_step":"0.1",
        "instruments":{"1234":{"sector":"technology","correlation_group":"equities",
            "gap_stress_pct":"5", "minimum_risk_pct":"0.1",
            "entry_windows":[{"starts_at":"2020-01-01T00:00:00Z","ends_at":"2100-01-01T00:00:00Z"}],
            "blackouts":[],"valid_until":"2100-01-01T00:00:00Z"}}
    })
}
pub fn budget() -> Value {
    json!({"max_daily_loss":"500","max_daily_turnover":"100000","max_daily_entries":100,"reentry_cooldown_seconds":0})
}
pub fn margin() -> BrokerMargin {
    BrokerMargin {
        observed_at: Utc::now(),
        maintenance_margin: Decimal::ZERO,
        excess_liquidity: Decimal::from(10000),
    }
}
pub fn market(scope: BrokerAccountScope, id: &str) -> RiskMarketData {
    let now = Utc::now();
    RiskMarketData {
        scope,
        instrument_id: id.into(),
        observed_at: now,
        received_at: now,
        real_time: true,
        bid: Decimal::new(9999, 2),
        ask: Decimal::new(10001, 2),
        bid_size: Decimal::from(10000),
        ask_size: Decimal::from(10000),
        daily_bars: (1..=65)
            .rev()
            .map(|d| MarketBar {
                time: now - Duration::days(d),
                open: Decimal::from(100),
                high: Decimal::from(101),
                low: Decimal::from(99),
                close: Decimal::from(100),
                volume: 100000,
            })
            .collect(),
    }
}
pub fn summary() -> Value {
    let now = Utc::now().timestamp_millis();
    let mut v = json!({"accountcode":{"value":"DU_TEST"},"availablefunds":{"amount":"100000","currency":"USD","timestamp":now,"isNull":false},"netliquidation":{"currency":"USD"}});
    for (name, amount) in [
        ("maintmarginreq", "0"),
        ("excessliquidity", "10000"),
        ("maintmarginreq-s", "0"),
        ("excessliquidity-s", "10000"),
        ("maintmarginreq-c", "0"),
        ("excessliquidity-c", "0"),
    ] {
        v[name] = json!({"amount":amount,"currency":"USD","timestamp":now,"isNull":false});
    }
    v
}
pub fn quote() -> Value {
    json!([{"conid":"1234","_updated":Utc::now().timestamp_millis(),"6509":"RpB","84":"99.99","86":"100.01","88":"10000","85":"10000","55":"TEST"}])
}
pub fn history() -> Value {
    json!({"conid":"1234","data":(1..=15).rev().map(|d|json!({"t":(Utc::now()-Duration::days(d)).timestamp_millis(),"o":"100","h":"101","l":"99","c":"100","v":"100000"})).collect::<Vec<_>>()})
}
