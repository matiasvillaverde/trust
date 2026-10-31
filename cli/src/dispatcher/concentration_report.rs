//! Legacy concentration report command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use clap::ArgMatches;
use core::TrustFacade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use serde_json::Value;
use uuid::Uuid;

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn concentration_report(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    use crate::views::ConcentrationView;
    use core::calculators_concentration::{ConcentrationCalculator, MetadataField};
    use std::str::FromStr;

    // Get account ID if provided
    let account_id = if let Some(account_arg) = sub_matches.get_one::<String>("account") {
        match Uuid::from_str(account_arg) {
            Ok(id) => Some(id),
            Err(_) => {
                return Err(output::report_error(
                    format,
                    "invalid_account_id",
                    "Invalid account ID format",
                ));
            }
        }
    } else {
        None
    };

    // Check if open-only flag is set
    let open_only = sub_matches.get_flag("open-only");

    // Get all trades (or filter by account)
    let all_trades = if let Some(id) = account_id {
        // Get trades for specific account - need to get all statuses
        let mut trades = Vec::new();
        for status in model::Status::all() {
            if let Ok(mut status_trades) = trust.search_trades(id, status) {
                trades.append(&mut status_trades);
            }
        }
        trades
    } else {
        // Get trades for all accounts
        match trust.search_all_accounts() {
            Ok(accounts) => {
                let mut all_trades = Vec::new();
                for account in accounts {
                    for status in model::Status::all() {
                        if let Ok(mut trades) = trust.search_trades(account.id, status) {
                            all_trades.append(&mut trades);
                        }
                    }
                }
                all_trades
            }
            Err(e) => {
                return Err(output::report_error(
                    format,
                    "accounts_fetch_failed",
                    format!("Error fetching accounts: {e}"),
                ));
            }
        }
    };

    if all_trades.is_empty() {
        match format {
            ReportOutputFormat::Text => {
                println!("\nNo trades found for the specified criteria.\n")
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "concentration",
                    "format_version": 1,
                    "generated_at": chrono::Utc::now().to_rfc3339(),
                    "scope": { "account_id": account_id.map(|id| id.to_string()) },
                    "filters": { "open_only": open_only },
                    "data": {
                        "sector": { "total_risk": "0", "groups": [], "warnings": [] },
                        "asset_class": { "total_risk": "0", "groups": [], "warnings": [] }
                    },
                    "consistency": {
                        "sector_group_sum": "0",
                        "sector_group_sum_matches_total": true,
                        "asset_group_sum": "0",
                        "asset_group_sum_matches_total": true
                    }
                });
                output::print_json(&payload)?;
            }
        }
        return Ok(());
    }

    // Filter for open positions if requested
    let trades_to_analyze = if open_only {
        ConcentrationCalculator::filter_open_trades(&all_trades)
    } else {
        all_trades
    };

    if trades_to_analyze.is_empty() {
        match format {
            ReportOutputFormat::Text => {
                println!("\nNo open trades found for the specified criteria.\n");
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "concentration",
                    "format_version": 1,
                    "generated_at": chrono::Utc::now().to_rfc3339(),
                    "scope": { "account_id": account_id.map(|id| id.to_string()) },
                    "filters": { "open_only": open_only },
                    "data": {
                        "sector": { "total_risk": "0", "groups": [], "warnings": [] },
                        "asset_class": { "total_risk": "0", "groups": [], "warnings": [] }
                    },
                    "consistency": {
                        "sector_group_sum": "0",
                        "sector_group_sum_matches_total": true,
                        "asset_group_sum": "0",
                        "asset_group_sum_matches_total": true
                    }
                });
                output::print_json(&payload)?;
            }
        }
        return Ok(());
    }

    // Analyze concentration by sector
    let sector_analysis =
        ConcentrationCalculator::analyze_by_metadata(&trades_to_analyze, MetadataField::Sector);

    // Analyze concentration by asset class
    let asset_class_analysis =
        ConcentrationCalculator::analyze_by_metadata(&trades_to_analyze, MetadataField::AssetClass);

    if format == ReportOutputFormat::Text {
        ConcentrationView::display(sector_analysis, asset_class_analysis, open_only);
        return Ok(());
    }

    let sector_total = sector_analysis.groups.iter().fold(dec!(0), |acc, group| {
        acc.checked_add(group.current_open_risk).unwrap_or(acc)
    });
    let asset_total = asset_class_analysis
        .groups
        .iter()
        .fold(dec!(0), |acc, group| {
            acc.checked_add(group.current_open_risk).unwrap_or(acc)
        });
    let sector_groups =
        concentration_groups_json(&sector_analysis.groups, sector_analysis.total_risk);
    let asset_groups = concentration_groups_json(
        &asset_class_analysis.groups,
        asset_class_analysis.total_risk,
    );

    let payload = json!({
        "report": "concentration",
        "format_version": 1,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "scope": { "account_id": account_id.map(|id| id.to_string()) },
        "filters": { "open_only": open_only },
        "data": {
            "sector": {
                "total_risk": output::decimal_string(sector_analysis.total_risk),
                "groups": sector_groups,
                "warnings": warnings_json(&sector_analysis.concentration_warnings)
            },
            "asset_class": {
                "total_risk": output::decimal_string(asset_class_analysis.total_risk),
                "groups": asset_groups,
                "warnings": warnings_json(&asset_class_analysis.concentration_warnings)
            }
        },
        "consistency": {
            "sector_group_sum": output::decimal_string(sector_total),
            "sector_group_sum_matches_total": sector_total == sector_analysis.total_risk,
            "asset_group_sum": output::decimal_string(asset_total),
            "asset_group_sum_matches_total": asset_total == asset_class_analysis.total_risk
        }
    });
    output::print_json(&payload)?;

    Ok(())
}

pub(in crate::dispatcher) fn concentration_groups_json(
    groups: &[core::calculators_concentration::ConcentrationGroup],
    total_risk: Decimal,
) -> Vec<Value> {
    let mut sorted_groups = groups.to_vec();
    sorted_groups.sort_by(|a, b| {
        b.current_open_risk
            .cmp(&a.current_open_risk)
            .then_with(|| a.name.cmp(&b.name))
    });
    sorted_groups
        .into_iter()
        .map(|group| {
            let risk_share = if total_risk > dec!(0) {
                group
                    .current_open_risk
                    .checked_mul(dec!(100))
                    .and_then(|v| v.checked_div(total_risk))
                    .unwrap_or(dec!(0))
            } else {
                dec!(0)
            };
            json!({
                "name": group.name,
                "trade_count": group.trade_count,
                "total_capital_deployed": output::decimal_string(group.total_capital_deployed),
                "realized_pnl": output::decimal_string(group.realized_pnl),
                "current_open_risk": output::decimal_string(group.current_open_risk),
                "open_risk_share_percentage": output::decimal_string(risk_share)
            })
        })
        .collect()
}

pub(in crate::dispatcher) fn warnings_json(
    warnings: &[core::calculators_concentration::ConcentrationWarning],
) -> Vec<Value> {
    warnings
        .iter()
        .map(|warning| {
            json!({
                "group_name": warning.group_name,
                "risk_percentage": output::decimal_string(warning.risk_percentage),
                "level": format!("{:?}", warning.level)
            })
        })
        .collect()
}
