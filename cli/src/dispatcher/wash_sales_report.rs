//! Legacy wash sales report command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use chrono::Utc;
use clap::ArgMatches;
use core::services::WashSaleMatch;
use core::services::WashSaleReplacementAdjustment;
use core::services::WashSaleReport;
use core::TrustFacade;
use serde_json::json;
use serde_json::Value;
use uuid::Uuid;

pub(in crate::dispatcher) fn wash_sales_report(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let account_id = if let Some(raw) = sub_matches.get_one::<String>("account") {
        Some(context::resolve_account_arg(trust, raw, format)?.id)
    } else {
        None
    };
    let report = trust
        .wash_sale_report(account_id)
        .map_err(|error| output::report_error(format, "report_failed", error.to_string()))?;

    match format {
        ReportOutputFormat::Text => print_wash_sales_text(&report, account_id),
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "wash_sales",
                "format_version": 1,
                "generated_at": Utc::now().to_rfc3339(),
                "scope": { "account_id": account_id.map(|id| id.to_string()) },
                "data": {
                    "scanned_trade_count": report.scanned_trade_count,
                    "eligible_loss_trade_count": report.eligible_loss_trade_count,
                    "wash_sale_count": report.wash_sale_count,
                    "total_disallowed_loss": output::decimal_string(report.total_disallowed_loss),
                    "total_basis_adjustment": output::decimal_string(report.total_basis_adjustment),
                    "matches": report.matches.iter().map(wash_sale_match_json).collect::<Vec<Value>>(),
                    "replacement_adjustments": report.replacement_adjustments.iter().map(wash_sale_adjustment_json).collect::<Vec<Value>>()
                },
                "consistency": {
                    "basis_adjustments_match_disallowed_loss": report.total_basis_adjustment == report.total_disallowed_loss,
                    "match_count_balanced": report.wash_sale_count == report.matches.len(),
                }
            });
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn print_wash_sales_text(
    report: &WashSaleReport,
    account_id: Option<Uuid>,
) {
    println!("Wash sale report");
    println!("================");
    match account_id {
        Some(id) => println!("account_id: {id}"),
        None => println!("account_id: all"),
    }
    println!("trades_scanned: {}", report.scanned_trade_count);
    println!("eligible_loss_trades: {}", report.eligible_loss_trade_count);
    println!("wash_sales: {}", report.wash_sale_count);
    println!(
        "total_disallowed_loss: {}",
        output::decimal_string(report.total_disallowed_loss)
    );
    println!(
        "total_basis_adjustment: {}",
        output::decimal_string(report.total_basis_adjustment)
    );
    if report.matches.is_empty() {
        println!("No wash sales detected.");
        return;
    }
    for wash_sale in &report.matches {
        println!(
            "{} sale={} replacement={} qty={} disallowed_loss={} adjusted_basis={}",
            wash_sale.symbol,
            wash_sale.loss_trade_id,
            wash_sale.replacement_trade_id,
            output::decimal_string(wash_sale.matched_quantity),
            output::decimal_string(wash_sale.disallowed_loss),
            output::decimal_string(wash_sale.adjusted_replacement_cost_basis)
        );
    }
}

pub(in crate::dispatcher) fn wash_sale_match_json(wash_sale: &WashSaleMatch) -> Value {
    json!({
        "account_id": wash_sale.account_id.to_string(),
        "symbol": wash_sale.symbol.clone(),
        "loss_trade_id": wash_sale.loss_trade_id.to_string(),
        "replacement_trade_id": wash_sale.replacement_trade_id.to_string(),
        "sale_date": wash_sale.sale_date.to_string(),
        "replacement_purchase_date": wash_sale.replacement_purchase_date.to_string(),
        "loss_quantity": output::decimal_string(wash_sale.loss_quantity),
        "matched_quantity": output::decimal_string(wash_sale.matched_quantity),
        "realized_loss": output::decimal_string(wash_sale.realized_loss),
        "disallowed_loss": output::decimal_string(wash_sale.disallowed_loss),
        "replacement_cost_basis": output::decimal_string(wash_sale.replacement_cost_basis),
        "adjusted_replacement_cost_basis": output::decimal_string(wash_sale.adjusted_replacement_cost_basis),
    })
}

pub(in crate::dispatcher) fn wash_sale_adjustment_json(
    adjustment: &WashSaleReplacementAdjustment,
) -> Value {
    json!({
        "replacement_trade_id": adjustment.replacement_trade_id.to_string(),
        "account_id": adjustment.account_id.to_string(),
        "symbol": adjustment.symbol.clone(),
        "replacement_purchase_date": adjustment.replacement_purchase_date.to_string(),
        "matched_quantity": output::decimal_string(adjustment.matched_quantity),
        "original_cost_basis": output::decimal_string(adjustment.original_cost_basis),
        "basis_adjustment": output::decimal_string(adjustment.basis_adjustment),
        "adjusted_cost_basis": output::decimal_string(adjustment.adjusted_cost_basis),
    })
}
