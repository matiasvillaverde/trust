//! Legacy timeline report command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::metrics_report;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use chrono::Datelike;
use chrono::Utc;
use clap::ArgMatches;
use core::TrustFacade;
use rust_decimal::Decimal;
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeMap;

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn timeline_report(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let account_raw = sub_matches
        .get_one::<String>("account")
        .ok_or_else(|| output::report_error(format, "missing_argument", "Missing --account"))?;
    let account = context::resolve_account_arg(trust, account_raw, format)?;
    let granularity = sub_matches
        .get_one::<String>("granularity")
        .ok_or_else(|| output::report_error(format, "missing_argument", "Missing --granularity"))?;
    if !matches!(granularity.as_str(), "day" | "week" | "month") {
        return Err(output::report_error(
            format,
            "invalid_argument",
            "Invalid --granularity value",
        ));
    }
    let (from, to) = metrics_report::parse_required_date_range(sub_matches, format)?;

    let txs = trust
        .get_account_transactions(account.id)
        .map_err(|e| output::report_error(format, "report_failed", e.to_string()))?;
    let scoped: Vec<model::Transaction> = txs
        .into_iter()
        .filter(|tx| {
            let day = tx.created_at.date();
            day >= from && day <= to
        })
        .collect();

    let mut buckets: BTreeMap<String, (u64, Decimal)> = BTreeMap::new();
    for tx in &scoped {
        let day = tx.created_at.date();
        let key = match granularity.as_str() {
            "day" => day.format("%Y-%m-%d").to_string(),
            "week" => {
                let iso = day.iso_week();
                format!("{}-W{:02}", iso.year(), iso.week())
            }
            "month" => day.format("%Y-%m").to_string(),
            _ => unreachable!("validated --granularity before iterating"),
        };
        let signed = metrics_report::transaction_cashflow_delta(tx);
        let entry = buckets.entry(key).or_insert((0, Decimal::ZERO));
        entry.0 = entry.0.saturating_add(1);
        entry.1 = entry.1.checked_add(signed).unwrap_or(entry.1);
    }

    let total_net = buckets.values().fold(Decimal::ZERO, |acc, (_, net)| {
        acc.checked_add(*net).unwrap_or(acc)
    });

    match format {
        ReportOutputFormat::Text => {
            println!("Timeline report");
            println!("===============");
            println!("account_id: {}", account.id);
            println!("granularity: {granularity}");
            println!("from: {from}");
            println!("to: {to}");
            println!("bucket_count: {}", buckets.len());
            println!(
                "net_cash_flow: {}",
                crate::zen::amount_share(total_net, total_net)
            );
            for (bucket, (count, net)) in &buckets {
                println!(
                    "{bucket}: events={count} net_cash_flow={}",
                    crate::zen::amount_share(*net, total_net)
                );
            }
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "timeline",
                "schema_version": 1,
                "generated_at": Utc::now().to_rfc3339(),
                "source": { "broker": account.broker_kind.as_str(), "provider": "trust_core" },
                "scope": {
                    "account_id": account.id.to_string(),
                    "granularity": granularity,
                    "from": from.to_string(),
                    "to": to.to_string(),
                },
                "data": {
                    "bucket_count": buckets.len(),
                    "net_cash_flow": output::decimal_string(total_net),
                    "buckets": buckets.iter().map(|(bucket, (count, net))| json!({
                        "bucket": bucket,
                        "event_count": count,
                        "net_cash_flow": output::decimal_string(*net),
                    })).collect::<Vec<Value>>()
                },
                "consistency": {
                    "sum_bucket_net": output::decimal_string(total_net),
                    "bucket_total_matches": total_net == buckets
                        .values()
                        .fold(Decimal::ZERO, |acc, (_, net)| acc.checked_add(*net).unwrap_or(acc))
                }
            });
            output::print_json(&payload)?;
        }
    }

    Ok(())
}
