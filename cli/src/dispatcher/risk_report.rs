//! Legacy risk report command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use clap::ArgMatches;
use core::TrustFacade;
use rust_decimal_macros::dec;
use serde_json::json;
use serde_json::Value;
use uuid::Uuid;

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn risk_report(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    use crate::views::RiskView;
    use core::calculators_risk::CapitalAtRiskCalculator;
    use model::Currency;
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

    // Calculate open positions
    let positions = match trust.calculate_open_positions(account_id) {
        Ok(p) => p,
        Err(e) => {
            return Err(output::report_error(
                format,
                "open_positions_failed",
                format!("Error calculating open positions: {e}"),
            ));
        }
    };

    // Calculate total capital at risk
    let total_capital_at_risk =
        match CapitalAtRiskCalculator::calculate_total_capital_at_risk(&positions) {
            Ok(total) => total,
            Err(e) => {
                return Err(output::report_error(
                    format,
                    "capital_at_risk_failed",
                    format!("Error calculating total capital at risk: {e}"),
                ));
            }
        };

    // Get account balance for equity calculation.
    let account_balance = if let Some(id) = account_id {
        match trust.search_balance(id, &Currency::USD) {
            Ok(balance) => balance.total_balance,
            Err(_) => {
                return Err(output::report_error(
                    format,
                    "account_balance_unavailable",
                    "Unable to retrieve account balance",
                ));
            }
        }
    } else {
        match trust.search_all_accounts() {
            Ok(accounts) => accounts
                .iter()
                .map(|account| trust.search_balance(account.id, &Currency::USD))
                .filter_map(Result::ok)
                .fold(dec!(0), |acc, balance| {
                    acc.checked_add(balance.total_balance).unwrap_or(acc)
                }),
            Err(_) => dec!(0),
        }
    };

    if format == ReportOutputFormat::Text {
        RiskView::display(positions, total_capital_at_risk, account_balance);
        return Ok(());
    }

    let position_sum = positions.iter().fold(dec!(0), |acc, position| {
        acc.checked_add(position.capital_amount).unwrap_or(acc)
    });
    let risk_percent = if account_balance > dec!(0) {
        total_capital_at_risk
            .checked_mul(dec!(100))
            .and_then(|v| v.checked_div(account_balance))
            .unwrap_or(dec!(0))
    } else {
        dec!(0)
    };
    let positions_json: Vec<Value> = positions
        .iter()
        .map(|position| {
            json!({
                "trade_id": position.trade_id.to_string(),
                "symbol": position.symbol,
                "capital_amount": output::decimal_string(position.capital_amount),
                "status": format!("{:?}", position.status),
                "funded_date": position.funded_date.format("%Y-%m-%dT%H:%M:%S").to_string()
            })
        })
        .collect();
    let payload = json!({
        "report": "risk",
        "format_version": 1,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "scope": { "account_id": account_id.map(|id| id.to_string()) },
        "data": {
            "account_equity": output::decimal_string(account_balance),
            "total_capital_at_risk": output::decimal_string(total_capital_at_risk),
            "capital_at_risk_percentage": output::decimal_string(risk_percent),
            "open_positions_count": positions.len(),
            "open_positions": positions_json
        },
        "consistency": {
            "position_sum": output::decimal_string(position_sum),
            "position_sum_matches_total": position_sum == total_capital_at_risk,
            "delta_total_minus_sum": output::decimal_string(
                total_capital_at_risk
                    .checked_sub(position_sum)
                    .unwrap_or(dec!(0))
                    .abs()
            )
        }
    });
    output::print_json(&payload)?;

    Ok(())
}
