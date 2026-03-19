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
use std::error::Error;

pub(crate) fn fetch_executions(
    client: &IbkrClient,
    trade: &Trade,
    account: &Account,
    after: Option<DateTime<Utc>>,
) -> Result<Vec<Execution>, Box<dyn Error>> {
    ensure_trade_account(trade, account)?;
    let rows = client.account_trades()?;
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
            parse_execution_side(&row, trade.category),
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

fn parse_execution_side(value: &serde_json::Value, trade_category: TradeCategory) -> ExecutionSide {
    let side = string_field_optional(value, "side")
        .unwrap_or_else(|| entry_side(trade_category).to_string())
        .to_ascii_lowercase();
    if side.contains("short") {
        return ExecutionSide::SellShort;
    }
    if side.starts_with('b') {
        return ExecutionSide::Buy;
    }
    ExecutionSide::Sell
}
