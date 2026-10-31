//! Legacy tax report command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::args;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use chrono::Utc;
use clap::ArgMatches;
use core::services::TaxAccountSummary;
use core::services::TaxRateOverrides;
use core::services::TaxReport;
use core::services::TaxTradeLine;
use core::TrustFacade;
use serde_json::json;
use serde_json::Value;
use uuid::Uuid;

pub(in crate::dispatcher) fn tax_report(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let account_id = if let Some(raw) = sub_matches.get_one::<String>("account") {
        Some(context::resolve_account_arg(trust, raw, format)?.id)
    } else {
        None
    };
    let rate_overrides = TaxRateOverrides {
        short_term: args::parse_optional_percent_arg(sub_matches, "short-term-rate", format)?,
        long_term: args::parse_optional_percent_arg(sub_matches, "long-term-rate", format)?,
    };
    let report = trust
        .tax_report(account_id, rate_overrides)
        .map_err(|error| output::report_error(format, "report_failed", error.to_string()))?;

    match format {
        ReportOutputFormat::Text => print_tax_text(&report, account_id),
        ReportOutputFormat::Json => {
            let account_trade_count = report
                .account_summaries
                .iter()
                .try_fold(0usize, |acc, summary| acc.checked_add(summary.trade_count))
                .unwrap_or(usize::MAX);
            let payload = json!({
                "report": "tax",
                "format_version": 1,
                "generated_at": Utc::now().to_rfc3339(),
                "scope": {
                    "account_id": account_id.map(|id| id.to_string()),
                    "short_term_rate_override": rate_overrides.short_term.map(output::decimal_string),
                    "long_term_rate_override": rate_overrides.long_term.map(output::decimal_string),
                },
                "data": {
                    "scanned_trade_count": report.scanned_trade_count,
                    "taxable_trade_count": report.taxable_trade_count,
                    "unclassified_trade_count": report.unclassified_trade_count,
                    "wash_sale_adjustment_count": report.wash_sale_adjustment_count,
                    "gross_realized_gain": output::decimal_string(report.gross_realized_gain),
                    "gross_realized_loss": output::decimal_string(report.gross_realized_loss),
                    "wash_sale_disallowed_loss": output::decimal_string(report.wash_sale_disallowed_loss),
                    "replacement_basis_adjustment": output::decimal_string(report.replacement_basis_adjustment),
                    "short_term_taxable_gain": output::decimal_string(report.short_term_taxable_gain),
                    "long_term_taxable_gain": output::decimal_string(report.long_term_taxable_gain),
                    "net_taxable_gain": output::decimal_string(report.net_taxable_gain),
                    "estimated_gross_tax_liability": output::decimal_string(report.estimated_gross_tax_liability),
                    "estimated_net_tax_liability": output::decimal_string(report.estimated_net_tax_liability),
                    "accounts": report.account_summaries.iter().map(tax_account_summary_json).collect::<Vec<Value>>(),
                    "trades": report.trades.iter().map(tax_trade_line_json).collect::<Vec<Value>>(),
                },
                "consistency": {
                    "account_trade_count_matches_total": account_trade_count == report.taxable_trade_count,
                    "net_gain_matches_components": report.short_term_taxable_gain
                        .checked_add(report.long_term_taxable_gain)
                        .map(|value| value == report.net_taxable_gain)
                        .unwrap_or(false),
                }
            });
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn print_tax_text(report: &TaxReport, account_id: Option<Uuid>) {
    println!("Tax report");
    println!("==========");
    match account_id {
        Some(id) => println!("account_id: {id}"),
        None => println!("account_id: all"),
    }
    println!("closed_trades_scanned: {}", report.scanned_trade_count);
    println!("classified_trades: {}", report.taxable_trade_count);
    println!(
        "net_taxable_gain: {}",
        output::decimal_string(report.net_taxable_gain)
    );
    println!(
        "estimated_net_tax_liability: {}",
        output::decimal_string(report.estimated_net_tax_liability)
    );
    println!(
        "wash_sale_disallowed_loss: {}",
        output::decimal_string(report.wash_sale_disallowed_loss)
    );
}

pub(in crate::dispatcher) fn tax_account_summary_json(summary: &TaxAccountSummary) -> Value {
    json!({
        "account_id": summary.account_id.to_string(),
        "short_term_rate": output::decimal_string(summary.short_term_rate),
        "long_term_rate": output::decimal_string(summary.long_term_rate),
        "trade_count": summary.trade_count,
        "gross_realized_gain": output::decimal_string(summary.gross_realized_gain),
        "gross_realized_loss": output::decimal_string(summary.gross_realized_loss),
        "wash_sale_disallowed_loss": output::decimal_string(summary.wash_sale_disallowed_loss),
        "replacement_basis_adjustment": output::decimal_string(summary.replacement_basis_adjustment),
        "short_term_taxable_gain": output::decimal_string(summary.short_term_taxable_gain),
        "long_term_taxable_gain": output::decimal_string(summary.long_term_taxable_gain),
        "net_taxable_gain": output::decimal_string(summary.net_taxable_gain),
        "estimated_gross_tax_liability": output::decimal_string(summary.estimated_gross_tax_liability),
        "estimated_net_tax_liability": output::decimal_string(summary.estimated_net_tax_liability),
    })
}

pub(in crate::dispatcher) fn tax_trade_line_json(line: &TaxTradeLine) -> Value {
    json!({
        "account_id": line.account_id.to_string(),
        "trade_id": line.trade_id.to_string(),
        "symbol": line.symbol.clone(),
        "opened_date": line.opened_date.to_string(),
        "closed_date": line.closed_date.to_string(),
        "holding_days": line.holding_days,
        "holding_period": line.holding_period.as_str(),
        "realized_gain_loss": output::decimal_string(line.realized_gain_loss),
        "wash_sale_disallowed_loss": output::decimal_string(line.wash_sale_disallowed_loss),
        "replacement_basis_adjustment": output::decimal_string(line.replacement_basis_adjustment),
        "taxable_gain_loss": output::decimal_string(line.taxable_gain_loss),
        "tax_rate": output::decimal_string(line.tax_rate),
        "estimated_tax_liability": output::decimal_string(line.estimated_tax_liability),
    })
}
