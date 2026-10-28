//! Conservative Client Portal whole-account observations. No inferred FX or margin.
use crate::{client::IbkrClient, parsing::string_field_optional, support::broker_account_id};
use chrono::{DateTime, Utc};
use model::{account_safety::*, Account, BrokerKind};
use rust_decimal::Decimal;
use serde_json::Value;
use std::{collections::BTreeSet, error::Error};

fn decimal(value: &Value, key: &str) -> Result<Decimal, Box<dyn Error>> {
    let field = value.get(key).ok_or("Missing account snapshot amount")?;
    let text = match field {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return Err("Invalid account snapshot amount".into()),
    };
    if text.contains(['e', 'E', ',']) {
        return Err("Ambiguous account snapshot amount".into());
    }
    Ok(Decimal::from_str_exact(&text)?)
}
fn text(value: &Value, key: &str) -> Result<String, Box<dyn Error>> {
    string_field_optional(value, key)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Missing account snapshot identity".into())
}

pub(crate) fn snapshot(account: &Account) -> Result<BrokerAccountSnapshot, Box<dyn Error>> {
    if account.broker_kind != BrokerKind::Ibkr {
        return Err("Broker identity mismatch".into());
    }
    let scope = BrokerAccountScope::from_account(account)?;
    let account_id = broker_account_id(account)?;
    let client = IbkrClient::for_account(account)?;
    client.prepare_trading_session(Some(account))?;
    let accounts = client.get_json_value("/portfolio/accounts", &[])?;
    if !accounts.as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            row.get("accountId").and_then(Value::as_str) == Some(account_id)
                || row.get("id").and_then(Value::as_str) == Some(account_id)
        })
    }) {
        return Err("Portfolio account identity is unavailable".into());
    }
    let first_positions = positions(&client, account_id)?;
    let first_orders = client.live_orders(account)?;
    let summary = client.get_json_value(&format!("/portfolio/{account_id}/summary"), &[])?;
    let ledger = client.get_json_value(&format!("/portfolio/{account_id}/ledger"), &[])?;
    let last_positions = positions(&client, account_id)?;
    let last_orders = client.live_orders(account)?;
    normalize(
        scope,
        &summary,
        &ledger,
        first_positions,
        last_positions,
        &first_orders,
        &last_orders,
        Utc::now(),
    )
}

fn positions(client: &IbkrClient, account: &str) -> Result<Vec<BrokerPosition>, Box<dyn Error>> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for page in 0..20 {
        let response =
            client.get_json_value(&format!("/portfolio/{account}/positions/{page}"), &[])?;
        let rows = response
            .as_array()
            .ok_or("Incomplete portfolio positions")?;
        if rows.len() > 100 {
            return Err("Invalid portfolio page size".into());
        }
        for row in rows {
            if row.get("acctId").and_then(Value::as_str) != Some(account) {
                return Err("Portfolio position account mismatch".into());
            }
            let instrument_id = text(row, "conid")?;
            if !seen.insert(instrument_id.clone()) {
                return Err("Duplicate portfolio position across pages".into());
            }
            result.push(BrokerPosition {
                instrument_id,
                symbol: string_field_optional(row, "ticker")
                    .or_else(|| string_field_optional(row, "contractDesc"))
                    .ok_or("Missing position symbol")?,
                currency: text(row, "currency")?,
                asset_class: text(row, "assetClass")?,
                quantity: decimal(row, "position")?,
                market_value: decimal(row, "mktValue")?,
            });
        }
        if rows.len() < 100 {
            result.sort_by(|a, b| a.instrument_id.cmp(&b.instrument_id));
            return Ok(result);
        }
    }
    Err("Portfolio positions reached completeness limit".into())
}

fn orders(rows: &[Value], account: &str) -> Result<Vec<BrokerWorkingOrder>, Box<dyn Error>> {
    let mut result = Vec::new();
    for row in rows {
        let owner = string_field_optional(row, "acct")
            .or_else(|| string_field_optional(row, "acctId"))
            .or_else(|| string_field_optional(row, "account"));
        let Some(owner) = owner else {
            return Err("Working order account is unavailable".into());
        };
        if owner != account {
            continue;
        }
        let status = text(row, "status")?.to_lowercase();
        if matches!(
            status.as_str(),
            "filled" | "cancelled" | "canceled" | "inactive" | "rejected" | "expired"
        ) {
            continue;
        }
        if !matches!(
            status.as_str(),
            "submitted"
                | "presubmitted"
                | "pre_submitted"
                | "pendingsubmit"
                | "pendingcancel"
                | "partially_filled"
                | "partiallyfilled"
                | "pending_submit"
                | "pending_cancel"
        ) {
            return Err("Unknown working order state".into());
        }
        let remaining = if row.get("remainingQuantity").is_some() {
            decimal(row, "remainingQuantity")?
        } else {
            decimal(row, "totalSize")?
                .checked_sub(decimal(row, "filledQuantity")?)
                .ok_or("Working order quantity overflow")?
        };
        let prices = order_prices(row, remaining)?;
        result.push(BrokerWorkingOrder {
            order_id: text(row, "orderId")?,
            client_order_id: string_field_optional(row, "order_ref"),
            instrument_id: text(row, "conid")?,
            symbol: text(row, "ticker")?,
            currency: text(row, "currency")?,
            asset_class: string_field_optional(row, "secType")
                .or_else(|| string_field_optional(row, "assetClass"))
                .ok_or("Working order asset class unavailable")?,
            side: text(row, "side")?.to_lowercase(),
            order_type: prices.order_type.into(),
            stop_price: prices.stop_price,
            active: matches!(
                status.as_str(),
                "submitted"
                    | "presubmitted"
                    | "pre_submitted"
                    | "partially_filled"
                    | "partiallyfilled"
            ),
            remaining_quantity: remaining,
            notional: prices.notional,
        });
    }
    result.sort_by(|a, b| a.order_id.cmp(&b.order_id));
    Ok(result)
}

struct OrderPrices {
    order_type: &'static str,
    stop_price: Option<Decimal>,
    notional: Option<Decimal>,
}
fn order_prices(row: &Value, remaining: Decimal) -> Result<OrderPrices, Box<dyn Error>> {
    let raw_type = string_field_optional(row, "orderType")
        .or_else(|| string_field_optional(row, "order_type"))
        .unwrap_or_default();
    let order_type = match raw_type.as_str() {
        "LMT" | "Limit" => "limit",
        "MKT" | "Market" => "market",
        "STP" | "Stop" => "stop",
        "STP LMT" | "Stop Limit" => "stop_limit",
        _ => "unknown",
    };
    let stop_price = if matches!(order_type, "stop" | "stop_limit") {
        // A stop-limit's regular price is its limit, never evidence of a trigger.
        ["auxPrice", "stopPrice"]
            .into_iter()
            .find(|key| row.get(*key).is_some())
            .or_else(|| (order_type == "stop" && row.get("price").is_some()).then_some("price"))
            .map(|key| decimal(row, key))
            .transpose()?
    } else {
        None
    };
    let notional = if matches!(order_type, "limit" | "stop_limit") {
        decimal(row, "price")
            .ok()
            .filter(|p| *p > Decimal::ZERO)
            .and_then(|p| remaining.checked_mul(p))
    } else {
        None
    };
    Ok(OrderPrices {
        order_type,
        stop_price,
        notional,
    })
}

#[allow(clippy::too_many_arguments)]
fn normalize(
    scope: BrokerAccountScope,
    summary: &Value,
    ledger: &Value,
    first_positions: Vec<BrokerPosition>,
    positions: Vec<BrokerPosition>,
    first_orders: &[Value],
    last_orders: &[Value],
    received_at: DateTime<Utc>,
) -> Result<BrokerAccountSnapshot, Box<dyn Error>> {
    if summary
        .pointer("/accountcode/value")
        .and_then(Value::as_str)
        != Some(scope.account_id.as_str())
    {
        return Err("Summary account identity mismatch".into());
    }
    let funds = summary
        .get("availablefunds")
        .ok_or("Available funds missing")?;
    if funds.get("isNull").and_then(Value::as_bool) == Some(true) {
        return Err("Available funds unavailable".into());
    }
    let currency = text(funds, "currency")?;
    let base = ledger.get("BASE").ok_or("Base account ledger missing")?;
    if base.get("acctcode").and_then(Value::as_str) != Some(scope.account_id.as_str()) {
        return Err("Ledger account identity mismatch".into());
    }
    // BASE is not a currency; require its currency from the aggregate summary.
    if summary
        .pointer("/netliquidation/currency")
        .and_then(Value::as_str)
        != Some(currency.as_str())
    {
        return Err("Base currency unavailable".into());
    }
    let ledger_time = DateTime::from_timestamp(
        base.get("timestamp")
            .and_then(Value::as_i64)
            .ok_or("Ledger timestamp missing")?,
        0,
    )
    .ok_or("Invalid ledger timestamp")?;
    let funds_time = DateTime::from_timestamp_millis(
        funds
            .get("timestamp")
            .and_then(Value::as_i64)
            .ok_or("Summary timestamp missing")?,
    )
    .ok_or("Invalid summary timestamp")?;
    let working_orders = orders(last_orders, &scope.account_id)?;
    let first = orders(first_orders, &scope.account_id)?;
    let stable_positions = first_positions
        .iter()
        .map(|p| (&p.instrument_id, p.quantity))
        .eq(positions.iter().map(|p| (&p.instrument_id, p.quantity)));
    let margin = margin(summary, &currency)?;
    Ok(BrokerAccountSnapshot {
        scope,
        currency,
        observed_at: ledger_time.min(funds_time),
        received_at,
        complete: true,
        consistent: stable_positions && first == working_orders,
        equity: decimal(base, "netliquidationvalue")?,
        cash: decimal(base, "cashbalance")?,
        available_funds: decimal(funds, "amount")?,
        margin,
        realized_pnl: decimal(base, "realizedpnl")?,
        unrealized_pnl: decimal(base, "unrealizedpnl")?,
        positions,
        working_orders,
    })
}

/// Supported admission products are securities/spot products, not commodity margin.
/// Use the smaller aggregate/securities cushion; never net a deficit in the other segment.
fn margin(
    summary: &Value,
    currency: &str,
) -> Result<Option<model::portfolio_risk::BrokerMargin>, Box<dyn Error>> {
    let fields = [
        "maintmarginreq",
        "excessliquidity",
        "maintmarginreq-s",
        "excessliquidity-s",
        "maintmarginreq-c",
        "excessliquidity-c",
    ];
    if fields.iter().any(|key| summary.get(*key).is_none()) {
        return Ok(None);
    }
    let mut values = std::collections::BTreeMap::new();
    let mut observed = None;
    for key in fields {
        let field = summary.get(key).ok_or("Margin evidence missing")?;
        if field.get("isNull").and_then(Value::as_bool) != Some(false)
            || field.get("currency").and_then(Value::as_str) != Some(currency)
        {
            return Err("Margin currency or availability is unverified".into());
        }
        let time = DateTime::from_timestamp_millis(
            field
                .get("timestamp")
                .and_then(Value::as_i64)
                .ok_or("Margin timestamp missing")?,
        )
        .ok_or("Margin timestamp invalid")?;
        observed = Some(observed.map_or(time, |previous: DateTime<Utc>| previous.min(time)));
        let value = decimal(field, "amount")?;
        if key.starts_with("maintmarginreq") && value < Decimal::ZERO {
            return Err("Negative maintenance margin evidence".into());
        }
        values.insert(key, value);
    }
    let amount = |key| values.get(key).copied().ok_or("Margin amount unavailable");
    if amount("maintmarginreq-c")? != Decimal::ZERO || amount("excessliquidity-c")? < Decimal::ZERO
    {
        return Err("Commodity segment margin is unsupported or deficient".into());
    }
    Ok(Some(model::portfolio_risk::BrokerMargin {
        observed_at: observed.ok_or("Margin timestamp missing")?,
        maintenance_margin: amount("maintmarginreq")?.max(amount("maintmarginreq-s")?),
        excess_liquidity: amount("excessliquidity")?.min(amount("excessliquidity-s")?),
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> (BrokerAccountScope, Value, Value) {
        (
            BrokerAccountScope {
                broker: "ibkr".into(),
                environment: "paper".into(),
                account_id: "DU_TEST".into(),
            },
            json!({"accountcode":{"value":"DU_TEST"},"availablefunds":{"currency":"USD","amount":"10000.1234567890123456789","timestamp":Utc::now().timestamp_millis(),"isNull":false},"netliquidation":{"currency":"USD"}}),
            json!({"BASE":{"acctcode":"DU_TEST","timestamp":Utc::now().timestamp(),"netliquidationvalue":"10000","cashbalance":"9000","realizedpnl":"10.25","unrealizedpnl":"-50.50"}}),
        )
    }
    fn order() -> Value {
        json!({"acct":"DU_TEST","orderId":1,"conid":1234,"ticker":"TEST","currency":"USD","secType":"STK","side":"BUY","orderType":"LMT","status":"Submitted","price":"100.1234567890123456789","totalSize":"10","filledQuantity":"3"})
    }
    #[test]
    fn margin_uses_oldest_timestamp_and_conservative_securities_segment() {
        let now = Utc::now().timestamp_millis();
        let mut summary = json!({});
        for (key, amount) in [
            ("maintmarginreq", "200"),
            ("excessliquidity", "900"),
            ("maintmarginreq-s", "300"),
            ("excessliquidity-s", "800"),
            ("maintmarginreq-c", "0"),
            ("excessliquidity-c", "100"),
        ] {
            summary[key] = json!({"amount":amount,"currency":"USD","timestamp":now,"isNull":false});
        }
        summary["excessliquidity-s"]["timestamp"] = json!(now - 1000);
        let m = margin(&summary, "USD").unwrap().unwrap();
        assert_eq!(m.maintenance_margin, Decimal::from(300));
        assert_eq!(m.excess_liquidity, Decimal::from(800));
        assert_eq!(m.observed_at.timestamp_millis(), now - 1000);
        for (key, field, value) in [
            ("maintmarginreq-c", "amount", json!("1")),
            ("excessliquidity-c", "amount", json!("-1")),
            ("excessliquidity-s", "currency", json!("EUR")),
            ("maintmarginreq", "isNull", json!(true)),
            ("excessliquidity", "timestamp", Value::Null),
            ("excessliquidity", "amount", json!("NaN")),
        ] {
            let mut invalid = summary.clone();
            invalid[key][field] = value;
            assert!(margin(&invalid, "USD").is_err(), "{key}/{field}");
        }
        for key in [
            "maintmarginreq",
            "excessliquidity",
            "maintmarginreq-s",
            "excessliquidity-s",
            "maintmarginreq-c",
            "excessliquidity-c",
        ] {
            let mut missing = summary.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(margin(&missing, "USD").unwrap().is_none());
        }
    }

    #[test]
    fn snapshot_preserves_decimal_identity_timestamp_and_partial_order_notional() {
        let (scope, summary, ledger) = fixture();
        let orders = vec![order()];
        let result = normalize(
            scope,
            &summary,
            &ledger,
            vec![],
            vec![],
            &orders,
            &orders,
            Utc::now(),
        )
        .unwrap();
        assert!(result.complete && result.consistent);
        assert_eq!(
            result.available_funds.to_string(),
            "10000.1234567890123456789"
        );
        assert_eq!(result.working_orders[0].remaining_quantity.to_string(), "7");
        assert_eq!(
            result.working_orders[0].notional.unwrap().to_string(),
            "700.8641975230864197523"
        );
        assert!(result.observed_at <= result.received_at);
        let exact: Value = serde_json::from_str("{\"amount\":10000.1234567890123456789}").unwrap();
        assert_eq!(
            decimal(&exact, "amount").unwrap().to_string(),
            "10000.1234567890123456789"
        );
    }
    #[test]
    fn snapshot_rejects_missing_source_identity_time_currency_and_ambiguous_amounts() {
        let (scope, summary, ledger) = fixture();
        for path in [
            "/BASE/acctcode",
            "/BASE/timestamp",
            "/BASE/netliquidationvalue",
            "/BASE/unrealizedpnl",
        ] {
            let mut invalid = ledger.clone();
            *invalid.pointer_mut(path).unwrap() = Value::Null;
            assert!(normalize(
                scope.clone(),
                &summary,
                &invalid,
                vec![],
                vec![],
                &[],
                &[],
                Utc::now()
            )
            .is_err());
        }
        for path in [
            "/accountcode/value",
            "/availablefunds/currency",
            "/availablefunds/timestamp",
            "/netliquidation/currency",
        ] {
            let mut invalid = summary.clone();
            *invalid.pointer_mut(path).unwrap() = Value::Null;
            assert!(normalize(
                scope.clone(),
                &invalid,
                &ledger,
                vec![],
                vec![],
                &[],
                &[],
                Utc::now()
            )
            .is_err());
        }
        for value in [
            json!("1,000"),
            json!("1e3"),
            json!("NaN"),
            json!(true),
            json!(null),
        ] {
            assert!(decimal(&json!({"amount":value}), "amount").is_err());
        }
    }
    #[test]
    fn changing_working_orders_and_unknown_types_cannot_look_like_stable_bounded_exposure() {
        let (scope, summary, ledger) = fixture();
        let first = order();
        let mut changed = first.clone();
        changed["filledQuantity"] = json!("4");
        let result = normalize(
            scope,
            &summary,
            &ledger,
            vec![],
            vec![],
            std::slice::from_ref(&first),
            &[changed],
            Utc::now(),
        )
        .unwrap();
        assert!(!result.consistent);
        for kind in ["MKT", "STP", "UNKNOWN"] {
            let mut row = first.clone();
            row["orderType"] = json!(kind);
            assert!(orders(&[row], "DU_TEST").unwrap()[0].notional.is_none());
        }
        let mut row = first.clone();
        row["status"] = json!("NEW_STATUS");
        assert!(orders(&[row], "DU_TEST").is_err());
        let mut row = first;
        row.as_object_mut().unwrap().remove("acct");
        assert!(orders(&[row], "DU_TEST").is_err());
    }
}
