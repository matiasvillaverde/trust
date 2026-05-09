use crate::client::IbkrClient;
use crate::orders::{entry_side, tracked_order_refs};
use crate::parsing::{
    decimal_field_any, decimal_field_optional_any, string_field_optional, trade_timestamp,
};
use crate::{support::ensure_trade_account, BROKER_NAME};
use chrono::{DateTime, Utc};
use model::{
    Account, Execution, ExecutionSide, ExecutionSource, FeeActivity, Trade, TradeCategory,
};
use serde_json::Value;
use std::error::Error;

pub(crate) fn fetch_executions(
    client: &IbkrClient,
    trade: &Trade,
    account: &Account,
    after: Option<DateTime<Utc>>,
) -> Result<Vec<Execution>, Box<dyn Error>> {
    ensure_trade_account(trade, account)?;
    let rows = client.account_trades()?;
    executions_from_rows(rows, trade, account, after)
}

fn executions_from_rows(
    rows: Vec<Value>,
    trade: &Trade,
    account: &Account,
    after: Option<DateTime<Utc>>,
) -> Result<Vec<Execution>, Box<dyn Error>> {
    let refs = tracked_order_refs(trade);
    let mut executions = Vec::new();

    for row in rows {
        let symbol = string_field_optional(&row, "symbol")
            .or_else(|| string_field_optional(&row, "ticker"))
            .unwrap_or_default();
        if symbol != trade.trading_vehicle.symbol {
            continue;
        }

        let broker_order_id = string_field_optional(&row, "order_ref");
        let Some(order_ref) = broker_order_id.clone() else {
            continue;
        };
        if !refs.contains(&order_ref) {
            continue;
        }

        let executed_at = trade_timestamp(&row).ok_or("IBKR trade row missing timestamp")?;
        if let Some(after) = after {
            if executed_at.and_utc() <= after {
                continue;
            }
        }

        let broker_execution_id = string_field_optional(&row, "execution_id")
            .or_else(|| string_field_optional(&row, "exec_id"))
            .ok_or("IBKR trade row missing execution id")?;
        let qty = decimal_field_any(&row, &["size", "qty", "quantity"])?;
        let price = decimal_field_any(&row, &["price", "trade_price"])?;

        let mut execution = Execution::new(
            BROKER_NAME.to_string(),
            ExecutionSource::AccountActivities,
            account.id,
            broker_execution_id,
            Some(order_ref),
            symbol,
            parse_execution_side(&row, trade.category)?,
            qty,
            price,
            executed_at,
        );
        execution.raw_json = Some(row.to_string());
        executions.push(execution);
    }

    Ok(executions)
}

pub(crate) fn fetch_fee_activities(
    client: &IbkrClient,
    trade: &Trade,
    account: &Account,
    after: Option<DateTime<Utc>>,
) -> Result<Vec<FeeActivity>, Box<dyn Error>> {
    ensure_trade_account(trade, account)?;
    let rows = client.account_trades()?;
    fee_activities_from_rows(rows, trade, account, after)
}

fn fee_activities_from_rows(
    rows: Vec<Value>,
    trade: &Trade,
    account: &Account,
    after: Option<DateTime<Utc>>,
) -> Result<Vec<FeeActivity>, Box<dyn Error>> {
    let refs = tracked_order_refs(trade);
    let mut fees = Vec::new();

    for row in rows {
        let Some(order_ref) = string_field_optional(&row, "order_ref") else {
            continue;
        };
        if !refs.contains(&order_ref) {
            continue;
        }

        let Some(commission) = decimal_field_optional_any(&row, &["commission", "comm"]) else {
            continue;
        };
        if commission.is_zero() {
            continue;
        }

        let occurred_at = trade_timestamp(&row).ok_or("IBKR fee row missing timestamp")?;
        if let Some(after) = after {
            if occurred_at.and_utc() <= after {
                continue;
            }
        }

        let Some(execution_id) = string_field_optional(&row, "execution_id")
            .or_else(|| string_field_optional(&row, "exec_id"))
        else {
            continue;
        };

        fees.push(FeeActivity {
            broker: BROKER_NAME.to_string(),
            broker_activity_id: format!("{execution_id}:commission"),
            account_id: account.id,
            broker_order_id: Some(order_ref),
            symbol: string_field_optional(&row, "symbol")
                .or_else(|| string_field_optional(&row, "ticker")),
            activity_type: "commission".to_string(),
            amount: commission.abs(),
            occurred_at,
            raw_json: Some(row.to_string()),
        });
    }

    Ok(fees)
}

fn parse_execution_side(
    value: &serde_json::Value,
    trade_category: TradeCategory,
) -> Result<ExecutionSide, Box<dyn Error>> {
    let side = string_field_optional(value, "side")
        .unwrap_or_else(|| entry_side(trade_category).to_string())
        .to_ascii_lowercase();
    if side.contains("short") {
        return Ok(ExecutionSide::SellShort);
    }
    match side.as_str() {
        "b" | "bot" | "bought" | "buy" => Ok(ExecutionSide::Buy),
        "s" | "sld" | "sell" | "sold" => Ok(ExecutionSide::Sell),
        _ => Err(format!("IBKR execution side '{side}' is unrecognized").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{executions_from_rows, fee_activities_from_rows, parse_execution_side};
    use chrono::{DateTime, Utc};
    use model::{Account, ExecutionSide, Trade, TradeCategory};
    use rust_decimal_macros::dec;
    use serde_json::json;

    fn account_and_trade() -> (Account, Trade) {
        let account = Account::default();
        let mut trade = Trade {
            account_id: account.id,
            ..Trade::default()
        };
        trade.trading_vehicle.symbol = "AAPL".to_string();
        trade.entry.broker_order_id = Some("entry-ref".to_string());
        trade.target.broker_order_id = Some("target-ref".to_string());
        trade.safety_stop.broker_order_id = Some("stop-ref".to_string());
        (account, trade)
    }

    fn utc(timestamp: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(timestamp)
            .expect("timestamp should parse")
            .with_timezone(&Utc)
    }

    #[test]
    fn parse_execution_side_accepts_known_ibkr_aliases() {
        assert_eq!(
            parse_execution_side(&json!({ "side": "BOT" }), TradeCategory::Long).expect("buy side"),
            ExecutionSide::Buy
        );
        assert_eq!(
            parse_execution_side(&json!({ "side": "sld" }), TradeCategory::Long)
                .expect("sell side"),
            ExecutionSide::Sell
        );
        assert_eq!(
            parse_execution_side(&json!({ "side": "sell_short" }), TradeCategory::Short)
                .expect("short side"),
            ExecutionSide::SellShort
        );
    }

    #[test]
    fn parse_execution_side_accepts_all_documented_buy_and_sell_aliases() {
        for alias in ["b", "bot", "bought", "buy"] {
            assert_eq!(
                parse_execution_side(&json!({ "side": alias }), TradeCategory::Long)
                    .expect("buy alias should parse"),
                ExecutionSide::Buy
            );
        }

        for alias in ["s", "sld", "sell", "sold"] {
            assert_eq!(
                parse_execution_side(&json!({ "side": alias }), TradeCategory::Long)
                    .expect("sell alias should parse"),
                ExecutionSide::Sell
            );
        }
    }

    #[test]
    fn parse_execution_side_defaults_from_trade_category_and_rejects_unknown_text() {
        assert_eq!(
            parse_execution_side(&json!({}), TradeCategory::Long).expect("default long side"),
            ExecutionSide::Buy
        );
        assert_eq!(
            parse_execution_side(&json!({}), TradeCategory::Short).expect("default short side"),
            ExecutionSide::Sell
        );

        let error = parse_execution_side(&json!({ "side": "blocked" }), TradeCategory::Long)
            .expect_err("unknown side rejected");
        assert!(error.to_string().contains("unrecognized"));
    }

    #[test]
    fn execution_rows_filter_symbol_order_ref_and_after_then_map_execution() {
        let (account, trade) = account_and_trade();
        let rows = vec![
            json!({
                "exec_id": "exec-1",
                "order_ref": "entry-ref",
                "ticker": "AAPL",
                "side": "BOT",
                "qty": "10",
                "trade_price": "100.25",
                "trade_time": "20260318-15:45:00"
            }),
            json!({
                "exec_id": "old",
                "order_ref": "entry-ref",
                "symbol": "AAPL",
                "side": "BUY",
                "qty": "1",
                "price": "99",
                "trade_time": "20260318-15:00:00"
            }),
            json!({
                "exec_id": "other-symbol",
                "order_ref": "entry-ref",
                "symbol": "MSFT",
                "side": "BUY",
                "qty": "1",
                "price": "99",
                "trade_time": "20260318-15:45:00"
            }),
            json!({
                "exec_id": "other-order",
                "order_ref": "external-ref",
                "symbol": "AAPL",
                "side": "BUY",
                "qty": "1",
                "price": "99",
                "trade_time": "20260318-15:45:00"
            }),
            json!({
                "exec_id": "missing-order-ref",
                "symbol": "AAPL",
                "side": "BUY",
                "qty": "1",
                "price": "99",
                "trade_time": "20260318-15:45:00"
            }),
        ];

        let executions =
            executions_from_rows(rows, &trade, &account, Some(utc("2026-03-18T15:30:00Z")))
                .expect("execution rows should map");

        assert_eq!(executions.len(), 1);
        let execution = executions.first().expect("one execution should remain");
        assert_eq!(execution.broker_execution_id, "exec-1");
        assert_eq!(execution.broker_order_id.as_deref(), Some("entry-ref"));
        assert_eq!(execution.side, ExecutionSide::Buy);
        assert_eq!(execution.qty, dec!(10));
        assert_eq!(execution.price, dec!(100.25));
        assert!(execution
            .raw_json
            .as_deref()
            .unwrap_or_default()
            .contains("exec-1"));
    }

    #[test]
    fn execution_rows_report_missing_required_timestamp_and_execution_id() {
        let (account, trade) = account_and_trade();

        let timestamp_error = executions_from_rows(
            vec![json!({
                "exec_id": "exec-1",
                "order_ref": "entry-ref",
                "symbol": "AAPL",
                "side": "BUY",
                "qty": "10",
                "price": "100.25"
            })],
            &trade,
            &account,
            None,
        )
        .expect_err("missing timestamp should fail");
        assert!(timestamp_error.to_string().contains("missing timestamp"));

        let execution_id_error = executions_from_rows(
            vec![json!({
                "order_ref": "entry-ref",
                "symbol": "AAPL",
                "side": "BUY",
                "qty": "10",
                "price": "100.25",
                "trade_time": "20260318-15:45:00"
            })],
            &trade,
            &account,
            None,
        )
        .expect_err("missing execution id should fail");
        assert!(execution_id_error
            .to_string()
            .contains("missing execution id"));
    }

    #[test]
    fn execution_rows_report_missing_required_quantity_and_price() {
        let (account, trade) = account_and_trade();

        let quantity_error = executions_from_rows(
            vec![json!({
                "exec_id": "exec-1",
                "order_ref": "entry-ref",
                "symbol": "AAPL",
                "side": "BUY",
                "price": "100.25",
                "trade_time": "20260318-15:45:00"
            })],
            &trade,
            &account,
            None,
        )
        .expect_err("missing quantity should fail");
        assert!(quantity_error.to_string().contains("missing decimal field"));

        let price_error = executions_from_rows(
            vec![json!({
                "exec_id": "exec-1",
                "order_ref": "entry-ref",
                "symbol": "AAPL",
                "side": "BUY",
                "qty": "10",
                "trade_time": "20260318-15:45:00"
            })],
            &trade,
            &account,
            None,
        )
        .expect_err("missing price should fail");
        assert!(price_error.to_string().contains("missing decimal field"));
    }

    #[test]
    fn fee_rows_filter_noise_and_normalize_commission_amounts() {
        let (account, trade) = account_and_trade();
        let rows = vec![
            json!({
                "execution_id": "exec-1",
                "order_ref": "target-ref",
                "symbol": "AAPL",
                "commission": "-1.25",
                "trade_time": "20260318-15:45:00"
            }),
            json!({
                "execution_id": "missing-order-ref",
                "symbol": "AAPL",
                "commission": "-8.00",
                "trade_time": "20260318-15:45:00"
            }),
            json!({
                "exec_id": "exec-2",
                "order_ref": "stop-ref",
                "ticker": "AAPL",
                "comm": "2.50",
                "trade_time": "20260318-15:50:00"
            }),
            json!({
                "execution_id": "zero",
                "order_ref": "target-ref",
                "symbol": "AAPL",
                "commission": "0",
                "trade_time": "20260318-15:45:00"
            }),
            json!({
                "execution_id": "no-commission",
                "order_ref": "target-ref",
                "symbol": "AAPL",
                "trade_time": "20260318-15:45:00"
            }),
            json!({
                "execution_id": "other-order",
                "order_ref": "external-ref",
                "symbol": "AAPL",
                "commission": "-9.99",
                "trade_time": "20260318-15:45:00"
            }),
            json!({
                "execution_id": "old",
                "order_ref": "target-ref",
                "symbol": "AAPL",
                "commission": "-2",
                "trade_time": "20260318-15:00:00"
            }),
            json!({
                "order_ref": "target-ref",
                "symbol": "AAPL",
                "commission": "-3",
                "trade_time": "20260318-15:45:00"
            }),
        ];

        let fees =
            fee_activities_from_rows(rows, &trade, &account, Some(utc("2026-03-18T15:30:00Z")))
                .expect("fee rows should map");

        assert_eq!(fees.len(), 2);
        let fee = fees.first().expect("first fee should remain");
        assert_eq!(fee.broker_activity_id, "exec-1:commission");
        assert_eq!(fee.broker_order_id.as_deref(), Some("target-ref"));
        assert_eq!(fee.amount, dec!(1.25));
        assert_eq!(fee.activity_type, "commission");

        let fee = fees.get(1).expect("second fee should remain");
        assert_eq!(fee.broker_activity_id, "exec-2:commission");
        assert_eq!(fee.broker_order_id.as_deref(), Some("stop-ref"));
        assert_eq!(fee.symbol.as_deref(), Some("AAPL"));
        assert_eq!(fee.amount, dec!(2.50));
    }

    #[test]
    fn fee_rows_report_missing_timestamp_for_matching_commissions() {
        let (account, trade) = account_and_trade();

        let error = fee_activities_from_rows(
            vec![json!({
                "execution_id": "exec-1",
                "order_ref": "target-ref",
                "symbol": "AAPL",
                "commission": "-1.25"
            })],
            &trade,
            &account,
            None,
        )
        .expect_err("missing timestamp should fail");

        assert!(error.to_string().contains("missing timestamp"));
    }
}
