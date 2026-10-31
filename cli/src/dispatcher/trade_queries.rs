//! Legacy trade queries command responsibility. Business mutations go through TrustFacade.
use crate::dialogs::TradeSearchDialogBuilder;
use crate::dialogs::TradeWatchDialogBuilder;
use crate::dispatcher::args;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use crate::dispatcher::report_calculations;
use chrono::NaiveDate;
use chrono::Utc;
use clap::ArgMatches;
use core::TrustFacade;
use model::Status;
use model::Trade;
use serde_json::json;
use serde_json::Value;
use std::str::FromStr;
use uuid::Uuid;

pub(in crate::dispatcher) fn trade_audit(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let format = output::parse_report_format(sub_matches);
    let trade_id = args::parse_uuid_arg(sub_matches, "trade-id", format)?;
    let verification = trust.trade_audit_verification(trade_id).map_err(|error| {
        output::report_error(format, "audit_verification_failed", error.to_string())
    })?;
    let events = trust
        .trade_audit_events(trade_id)
        .map_err(|error| output::report_error(format, "audit_read_failed", error.to_string()))?;
    let envelope = json!({
        "schema_version": 1,
        "verification": &verification,
        "events": &events,
    });

    if let Some(output) = sub_matches.get_one::<String>("output") {
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .map_err(|error| {
                output::report_error(
                    format,
                    "audit_export_failed",
                    format!("Could not create audit export {output}: {error}"),
                )
            })?;
        let mut encoded = serde_json::to_vec_pretty(&envelope).map_err(|error| {
            output::report_error(format, "audit_export_failed", error.to_string())
        })?;
        encoded.push(b'\n');
        file.write_all(&encoded).map_err(|error| {
            output::report_error(format, "audit_export_failed", error.to_string())
        })?;
    }

    match format {
        ReportOutputFormat::Json => println!("{envelope}"),
        ReportOutputFormat::Text => {
            println!(
                "Audit verified: trade={} events={} head={}",
                verification.trade_id, verification.event_count, verification.head_hash
            );
            for event in events {
                println!(
                    "{} {} {} actor={} operation={}",
                    event.occurred_at,
                    event.event_type,
                    event.outcome,
                    event.actor,
                    event.operation_id
                );
            }
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn create_watch(trust: &mut TrustFacade, sub_matches: &ArgMatches) {
    let mut builder = TradeWatchDialogBuilder::new().account(trust);
    if sub_matches.get_flag("latest") {
        builder = builder.latest(trust);
    } else {
        builder = builder.search(trust);
    }
    builder.build(trust).display();
}

#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub(in crate::dispatcher) fn search_trade(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    if sub_matches.get_one::<String>("account").is_none()
        && sub_matches.get_one::<String>("status").is_none()
        && sub_matches.get_one::<String>("symbol").is_none()
        && sub_matches.get_one::<String>("from").is_none()
        && sub_matches.get_one::<String>("to").is_none()
    {
        TradeSearchDialogBuilder::new()
            .account(trust)
            .status()
            .show_balance()
            .search(trust)
            .display();
        return Ok(());
    }

    let format = output::parse_report_format(sub_matches);
    let account_id = match sub_matches.get_one::<String>("account") {
        Some(account_raw) => Some(context::resolve_account_arg(trust, account_raw, format)?.id),
        None => None,
    };
    let status_filter = match sub_matches.get_one::<String>("status") {
        Some(raw) => Some(Status::from_str(raw).map_err(|_| {
            output::report_error(
                format,
                "invalid_status",
                format!("Invalid --status value '{raw}'"),
            )
        })?),
        None => None,
    };
    let symbol_filter = sub_matches
        .get_one::<String>("symbol")
        .map(|value| value.trim().to_uppercase());
    let from = sub_matches
        .get_one::<String>("from")
        .map(|raw| NaiveDate::parse_from_str(raw, "%Y-%m-%d"))
        .transpose()
        .map_err(|_| output::report_error(format, "invalid_date", "Invalid --from date"))?;
    let to = sub_matches
        .get_one::<String>("to")
        .map(|raw| NaiveDate::parse_from_str(raw, "%Y-%m-%d"))
        .transpose()
        .map_err(|_| output::report_error(format, "invalid_date", "Invalid --to date"))?;

    if let (Some(from), Some(to)) = (from, to) {
        if from > to {
            return Err(output::report_error(
                format,
                "invalid_date_range",
                "--from must be less than or equal to --to",
            ));
        }
    }

    let open_statuses = [
        Status::New,
        Status::Funded,
        Status::Submitted,
        Status::PartiallyFilled,
        Status::Filled,
    ];
    let all_statuses = Status::all();
    let statuses: Vec<Status> = match status_filter {
        Some(status) => vec![status],
        None => all_statuses.clone(),
    };

    let mut trades: Vec<Trade> = Vec::new();
    if let Some(account_id) = account_id {
        for status in statuses {
            if let Ok(mut status_trades) = trust.search_trades(account_id, status) {
                trades.append(&mut status_trades);
            }
        }
    } else if status_filter.is_none() {
        for status in all_statuses {
            for trade in report_calculations::open_trades_for_scope(trust, None) {
                if trade.status == status {
                    trades.push(trade);
                }
            }
        }
    } else if let Ok(accounts) = trust.search_all_accounts() {
        for account in accounts {
            for status in statuses.clone() {
                if let Ok(mut status_trades) = trust.search_trades(account.id, status) {
                    trades.append(&mut status_trades);
                }
            }
        }
    }

    if status_filter.is_none() {
        trades.retain(|trade| {
            open_statuses.contains(&trade.status)
                || trade.status.to_string().contains("closed")
                || matches!(
                    trade.status,
                    Status::Canceled | Status::Expired | Status::Rejected
                )
        });
    }
    if let Some(symbol) = symbol_filter {
        trades.retain(|trade| trade.trading_vehicle.symbol.eq_ignore_ascii_case(&symbol));
    }
    if from.is_some() || to.is_some() {
        trades.retain(|trade| report_calculations::trade_in_window(trade, from, to));
    }
    trades.sort_by_key(|trade| trade.updated_at);
    trades.reverse();

    match format {
        ReportOutputFormat::Text => {
            println!("Trades found: {}", trades.len());
            for trade in trades.iter().take(50) {
                println!(
                    "{} {} {} {} qty={} updated_at={}",
                    trade.id,
                    trade.status,
                    trade.trading_vehicle.symbol,
                    trade.category,
                    crate::display_precision::format_quantity(
                        trade.trading_vehicle.category,
                        trade.entry.quantity
                    ),
                    trade.updated_at
                );
            }
            if trades.len() > 50 {
                println!(
                    "... ({} more trades omitted)",
                    trades.len().saturating_sub(50)
                );
            }
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "trade_search",
                "schema_version": 1,
                "generated_at": Utc::now().to_rfc3339(),
                "filters": {
                    "account": sub_matches.get_one::<String>("account"),
                    "status": sub_matches.get_one::<String>("status"),
                    "symbol": sub_matches.get_one::<String>("symbol"),
                    "from": sub_matches.get_one::<String>("from"),
                    "to": sub_matches.get_one::<String>("to"),
                },
                "data": {
                    "count": trades.len(),
                    "trades": trades.iter().map(|trade| json!({
                        "id": trade.id.to_string(),
                        "account_id": trade.account_id.to_string(),
                        "symbol": trade.trading_vehicle.symbol,
                        "status": trade.status.to_string(),
                        "category": trade.category.to_string(),
                        "quantity": trade.entry.quantity,
                        "entry_price": output::decimal_string(trade.entry.unit_price),
                        "stop_price": output::decimal_string(trade.safety_stop.unit_price),
                        "target_price": output::decimal_string(trade.target.unit_price),
                        "updated_at": trade.updated_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    })).collect::<Vec<Value>>(),
                }
            });
            output::print_json(&payload)?;
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn list_open_trades(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let format = output::parse_report_format(sub_matches);
    let account_id = match sub_matches.get_one::<String>("account") {
        Some(account_raw) => Some(context::resolve_account_arg(trust, account_raw, format)?.id),
        None => None,
    };
    let mut trades = report_calculations::open_trades_for_scope(trust, account_id);
    trades.sort_by_key(|trade| trade.updated_at);
    trades.reverse();

    match format {
        ReportOutputFormat::Text => {
            println!("Open trades: {}", trades.len());
            for trade in &trades {
                println!(
                    "{} {} {} qty={} updated_at={}",
                    trade.id,
                    trade.status,
                    trade.trading_vehicle.symbol,
                    crate::display_precision::format_quantity(
                        trade.trading_vehicle.category,
                        trade.entry.quantity
                    ),
                    trade.updated_at
                );
            }
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "trade_list_open",
                "schema_version": 1,
                "generated_at": Utc::now().to_rfc3339(),
                "scope": { "account_id": account_id.map(|id| id.to_string()) },
                "data": {
                    "count": trades.len(),
                    "trades": trades.iter().map(|trade| json!({
                        "id": trade.id.to_string(),
                        "account_id": trade.account_id.to_string(),
                        "symbol": trade.trading_vehicle.symbol,
                        "status": trade.status.to_string(),
                        "quantity": trade.entry.quantity,
                        "updated_at": trade.updated_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    })).collect::<Vec<Value>>()
                }
            });
            output::print_json(&payload)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn reconcile_trades(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let format = output::parse_report_format(sub_matches);
    let mut trades: Vec<Trade> = if let Some(trade_id_raw) =
        sub_matches.get_one::<String>("trade-id")
    {
        let trade_id = Uuid::parse_str(trade_id_raw).map_err(|_| {
            output::report_error(format, "invalid_trade_id", "Invalid --trade-id UUID")
        })?;
        vec![context::find_trade_by_id(
            trust,
            trade_id,
            &[Status::Submitted, Status::PartiallyFilled, Status::Filled],
            format,
        )?]
    } else {
        let account_id = match sub_matches.get_one::<String>("account") {
            Some(account_raw) => Some(context::resolve_account_arg(trust, account_raw, format)?.id),
            None => None,
        };
        report_calculations::open_trades_for_scope(trust, account_id)
            .into_iter()
            .filter(|trade| {
                matches!(
                    trade.status,
                    Status::Submitted | Status::PartiallyFilled | Status::Filled
                )
            })
            .collect()
    };

    trades.sort_by_key(|trade| trade.updated_at);
    trades.reverse();

    let mut successes: Vec<Value> = Vec::new();
    let mut failures: Vec<Value> = Vec::new();
    for trade in trades {
        match context::account_by_id(trust, trade.account_id, format) {
            Ok(account) => match trust.sync_trade(&trade, &account) {
                Ok((new_status, orders, _log)) => {
                    successes.push(json!({
                        "trade_id": trade.id.to_string(),
                        "status_before": trade.status.to_string(),
                        "status_after": new_status.to_string(),
                        "updated_orders": orders.len(),
                    }));
                }
                Err(error) => failures.push(json!({
                    "trade_id": trade.id.to_string(),
                    "error": error.to_string(),
                })),
            },
            Err(error) => failures.push(json!({
                "trade_id": trade.id.to_string(),
                "error": error.to_string(),
            })),
        }
    }

    match format {
        ReportOutputFormat::Text => {
            println!(
                "Reconcile completed: success={} failed={}",
                successes.len(),
                failures.len()
            );
            for entry in &successes {
                println!(
                    "OK {} {} -> {} orders={}",
                    entry["trade_id"].as_str().unwrap_or(""),
                    entry["status_before"].as_str().unwrap_or(""),
                    entry["status_after"].as_str().unwrap_or(""),
                    entry["updated_orders"].as_u64().unwrap_or(0)
                );
            }
            for entry in &failures {
                println!(
                    "ERR {} {}",
                    entry["trade_id"].as_str().unwrap_or(""),
                    entry["error"].as_str().unwrap_or("")
                );
            }
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "trade_reconcile",
                "schema_version": 1,
                "generated_at": Utc::now().to_rfc3339(),
                "data": {
                    "success_count": successes.len(),
                    "failure_count": failures.len(),
                    "successes": successes,
                    "failures": failures,
                }
            });
            output::print_json(&payload)?;
        }
    }
    Ok(())
}
