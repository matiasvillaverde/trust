//! Legacy attribution report command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::metrics_report;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use chrono::Utc;
use clap::ArgMatches;
use core::TrustFacade;
use model::Trade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn attribution_report(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let account_raw = sub_matches
        .get_one::<String>("account")
        .ok_or_else(|| output::report_error(format, "missing_argument", "Missing --account"))?;
    let account = context::resolve_account_arg(trust, account_raw, format)?;
    let by = sub_matches
        .get_one::<String>("by")
        .ok_or_else(|| output::report_error(format, "missing_argument", "Missing --by"))?;
    if !matches!(by.as_str(), "symbol" | "sector" | "asset-class") {
        return Err(output::report_error(
            format,
            "invalid_argument",
            "Invalid --by value",
        ));
    }
    let (from, to) = metrics_report::parse_required_date_range(sub_matches, format)?;

    let trades = trust
        .search_closed_trades(Some(account.id))
        .map_err(|e| output::report_error(format, "report_failed", e.to_string()))?;
    let in_window: Vec<Trade> = trades
        .into_iter()
        .filter(|trade| {
            let day = trade.updated_at.date();
            day >= from && day <= to
        })
        .collect();

    let mut groups: HashMap<String, (u64, Decimal, Decimal)> = HashMap::new();
    for trade in &in_window {
        let key = match by.as_str() {
            "symbol" => trade.trading_vehicle.symbol.clone(),
            "sector" => trade
                .sector
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            "asset-class" => trade
                .asset_class
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            _ => unreachable!("validated --by before iterating"),
        };
        let entry = groups
            .entry(key)
            .or_insert((0, Decimal::ZERO, Decimal::ZERO));
        entry.0 = entry.0.saturating_add(1);
        entry.1 = entry
            .1
            .checked_add(trade.balance.total_performance)
            .unwrap_or(entry.1);
        entry.2 = entry
            .2
            .checked_add(trade.balance.funding)
            .unwrap_or(entry.2);
    }

    let total_pnl = in_window.iter().fold(Decimal::ZERO, |acc, trade| {
        acc.checked_add(trade.balance.total_performance)
            .unwrap_or(acc)
    });
    let total_funding = in_window.iter().fold(Decimal::ZERO, |acc, trade| {
        acc.checked_add(trade.balance.funding).unwrap_or(acc)
    });
    let mut rows: Vec<(String, u64, Decimal, Decimal)> = groups
        .into_iter()
        .map(|(key, (count, pnl, funding))| (key, count, pnl, funding))
        .collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.2));

    match format {
        ReportOutputFormat::Text => {
            println!("Attribution report ({by})");
            println!("=======================");
            println!("account_id: {}", account.id);
            println!("from: {from}");
            println!("to: {to}");
            println!("closed_trades: {}", in_window.len());
            println!(
                "total_pnl: {}",
                crate::zen::amount_share(total_pnl, total_pnl)
            );
            for (key, count, pnl, funding) in &rows {
                let share = if total_pnl == Decimal::ZERO {
                    Decimal::ZERO
                } else {
                    pnl.checked_div(total_pnl)
                        .and_then(|v| v.checked_mul(dec!(100)))
                        .unwrap_or(Decimal::ZERO)
                };
                println!(
                    "{key}: trades={count} pnl={} funding={} share_pct={}",
                    crate::zen::amount_share(*pnl, total_pnl),
                    crate::zen::amount_share(*funding, total_funding),
                    output::decimal_string(share),
                );
            }
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "attribution",
                "schema_version": 1,
                "generated_at": Utc::now().to_rfc3339(),
                "source": { "broker": account.broker_kind.as_str(), "provider": "trust_core" },
                "scope": {
                    "account_id": account.id.to_string(),
                    "from": from.to_string(),
                    "to": to.to_string(),
                    "by": by,
                },
                "data": {
                    "closed_trade_count": in_window.len(),
                    "total_pnl": output::decimal_string(total_pnl),
                    "groups": rows.iter().map(|(key, count, pnl, funding)| {
                        let share = if total_pnl == Decimal::ZERO {
                            Decimal::ZERO
                        } else {
                            pnl.checked_div(total_pnl)
                                .and_then(|v| v.checked_mul(dec!(100)))
                                .unwrap_or(Decimal::ZERO)
                        };
                        json!({
                            "key": key,
                            "trade_count": count,
                            "pnl": output::decimal_string(*pnl),
                            "funding": output::decimal_string(*funding),
                            "share_pct": output::decimal_string(share),
                        })
                    }).collect::<Vec<Value>>()
                },
                "consistency": {
                    "sum_group_pnl": output::decimal_string(rows.iter().fold(Decimal::ZERO, |acc, row| acc.checked_add(row.2).unwrap_or(acc))),
                    "matches_total_pnl": rows.iter().fold(Decimal::ZERO, |acc, row| acc.checked_add(row.2).unwrap_or(acc)) == total_pnl
                }
            });
            output::print_json(&payload)?;
        }
    }

    Ok(())
}
