//! Legacy benchmark report command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::metrics_report;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use chrono::DateTime;
use chrono::Utc;
use clap::ArgMatches;
use core::TrustFacade;
use model::Trade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn benchmark_report(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let account_raw = sub_matches
        .get_one::<String>("account")
        .ok_or_else(|| output::report_error(format, "missing_argument", "Missing --account"))?;
    let account = context::resolve_account_arg(trust, account_raw, format)?;
    let benchmark_symbol = sub_matches
        .get_one::<String>("benchmark")
        .ok_or_else(|| output::report_error(format, "missing_argument", "Missing --benchmark"))?
        .trim()
        .to_uppercase();
    let (from, to) = metrics_report::parse_required_date_range(sub_matches, format)?;
    let start = DateTime::<Utc>::from_naive_utc_and_offset(
        from.and_hms_opt(0, 0, 0).unwrap_or_default(),
        Utc,
    );
    let end = DateTime::<Utc>::from_naive_utc_and_offset(
        to.and_hms_opt(23, 59, 59).unwrap_or_default(),
        Utc,
    );

    let closed = trust
        .search_closed_trades(Some(account.id))
        .map_err(|e| output::report_error(format, "report_failed", e.to_string()))?;
    let scoped: Vec<Trade> = closed
        .into_iter()
        .filter(|trade| {
            let day = trade.updated_at.date();
            day >= from && day <= to
        })
        .collect();

    let funding = scoped.iter().fold(Decimal::ZERO, |acc, trade| {
        acc.checked_add(trade.balance.funding).unwrap_or(acc)
    });
    let pnl = scoped.iter().fold(Decimal::ZERO, |acc, trade| {
        acc.checked_add(trade.balance.total_performance)
            .unwrap_or(acc)
    });
    let strategy_return_pct = if funding == Decimal::ZERO {
        Decimal::ZERO
    } else {
        pnl.checked_div(funding)
            .and_then(|v| v.checked_mul(dec!(100)))
            .unwrap_or(Decimal::ZERO)
    };

    let mut bars = trust
        .market_bars(
            &account,
            &benchmark_symbol,
            start,
            end,
            model::BarTimeframe::OneDay,
        )
        .map_err(|e| {
            output::report_error(
                format,
                "benchmark_data_failed",
                format!("Unable to retrieve benchmark bars: {e}"),
            )
        })?;
    bars.sort_by_key(|bar| bar.time);
    let benchmark_return_pct = if let (Some(first), Some(last)) = (bars.first(), bars.last()) {
        if first.open == Decimal::ZERO {
            Decimal::ZERO
        } else {
            last.close
                .checked_sub(first.open)
                .and_then(|v| v.checked_div(first.open))
                .and_then(|v| v.checked_mul(dec!(100)))
                .unwrap_or(Decimal::ZERO)
        }
    } else {
        Decimal::ZERO
    };
    let alpha_pct = strategy_return_pct
        .checked_sub(benchmark_return_pct)
        .unwrap_or(Decimal::ZERO);

    match format {
        ReportOutputFormat::Text => {
            println!("Benchmark report");
            println!("================");
            println!("account_id: {}", account.id);
            println!("benchmark: {benchmark_symbol}");
            println!("from: {from}");
            println!("to: {to}");
            println!("closed_trades: {}", scoped.len());
            println!(
                "strategy_return_pct: {}",
                output::decimal_string(strategy_return_pct)
            );
            println!(
                "benchmark_return_pct: {}",
                output::decimal_string(benchmark_return_pct)
            );
            println!("alpha_pct: {}", output::decimal_string(alpha_pct));
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "benchmark",
                "schema_version": 1,
                "generated_at": Utc::now().to_rfc3339(),
                "source": { "broker": account.broker_kind.as_str(), "provider": "trust_core" },
                "scope": {
                    "account_id": account.id.to_string(),
                    "benchmark": benchmark_symbol,
                    "from": from.to_string(),
                    "to": to.to_string(),
                },
                "data": {
                    "closed_trade_count": scoped.len(),
                    "strategy": {
                        "funding": output::decimal_string(funding),
                        "pnl": output::decimal_string(pnl),
                        "return_pct": output::decimal_string(strategy_return_pct),
                    },
                    "benchmark": {
                        "bars": bars.len(),
                        "return_pct": output::decimal_string(benchmark_return_pct),
                    },
                    "alpha_pct": output::decimal_string(alpha_pct),
                }
            });
            output::print_json(&payload)?;
        }
    }
    Ok(())
}
