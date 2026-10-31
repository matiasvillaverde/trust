//! Legacy levels command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_level_subcommand;
use crate::command_routing::LevelSubcommand;
use crate::dispatcher::args;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::level_output;
use crate::dispatcher::level_policy;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use clap::ArgMatches;
use core::services::leveling::LevelPerformanceSnapshot;
use core::TrustFacade;
use model::LevelTrigger;
use serde_json::json;
use std::str::FromStr;
use uuid::Uuid;

pub(in crate::dispatcher) fn dispatch_level(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let format = output::parse_report_format(sub_matches);
    match parse_level_subcommand(sub_matches) {
        LevelSubcommand::Status(sub_sub_matches) => level_status(trust, sub_sub_matches, format)?,
        LevelSubcommand::Triggers(sub_sub_matches) => {
            level_triggers(trust, sub_sub_matches, format)?
        }
        LevelSubcommand::History(sub_sub_matches) => level_history(trust, sub_sub_matches, format)?,
        LevelSubcommand::Change(sub_sub_matches) => level_change(trust, sub_sub_matches, format)?,
        LevelSubcommand::Evaluate(sub_sub_matches) => {
            level_evaluate(trust, sub_sub_matches, format)?
        }
        LevelSubcommand::Progress(sub_sub_matches) => {
            level_progress(trust, sub_sub_matches, format)?
        }
        LevelSubcommand::RulesShow(sub_sub_matches) => {
            level_policy::level_rules_show(trust, sub_sub_matches, format)?
        }
        LevelSubcommand::RulesSet(sub_sub_matches) => {
            level_policy::level_rules_set(trust, sub_sub_matches, format)?
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn resolve_level_account_id(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<Uuid, CliError> {
    if let Some(account_arg) = sub_matches.get_one::<String>("account") {
        return Uuid::from_str(account_arg).map_err(|_| {
            output::report_error(format, "invalid_account_id", "Invalid account ID format")
        });
    }

    let accounts = trust.search_all_accounts().map_err(|error| {
        output::report_error(
            format,
            "accounts_unavailable",
            format!("Failed to load accounts: {error}"),
        )
    })?;

    if let [account] = accounts.as_slice() {
        return Ok(account.id);
    }

    Err(output::report_error(
        format,
        "account_selection_required",
        "Use --account when multiple accounts exist",
    ))
}

pub(in crate::dispatcher) fn level_snapshot_from_args(
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<LevelPerformanceSnapshot, CliError> {
    Ok(LevelPerformanceSnapshot {
        profitable_trades: *sub_matches
            .get_one::<u32>("profitable-trades")
            .ok_or_else(|| {
                output::report_error(
                    format,
                    "missing_profitable_trades",
                    "Missing --profitable-trades",
                )
            })?,
        win_rate_percentage: args::parse_decimal_arg(sub_matches, "win-rate", format)?,
        monthly_loss_percentage: args::parse_decimal_arg(sub_matches, "monthly-loss", format)?,
        largest_loss_percentage: args::parse_decimal_arg(sub_matches, "largest-loss", format)?,
        consecutive_wins: *sub_matches
            .get_one::<u32>("consecutive-wins")
            .ok_or_else(|| {
                output::report_error(
                    format,
                    "missing_consecutive_wins",
                    "Missing --consecutive-wins",
                )
            })?,
    })
}

pub(in crate::dispatcher) fn level_triggers(
    _trust: &mut TrustFacade,
    _sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let triggers = model::LevelTrigger::known_values();

    match format {
        ReportOutputFormat::Text => {
            println!("Supported level triggers");
            println!("========================");
            for trigger in triggers {
                println!("- {trigger}");
            }
            println!("- custom values are accepted and normalized to lowercase");
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "level_triggers",
                "format_version": 1,
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "data": {
                    "supported": triggers,
                    "custom_allowed": true
                }
            });
            output::print_json(&payload)?;
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn level_status(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    use crate::views::LevelView;

    let account_id = resolve_level_account_id(trust, sub_matches, format)?;
    let level = trust.level_for_account(account_id).map_err(|error| {
        output::report_error(
            format,
            "level_status_failed",
            format!("Unable to load level status: {error}"),
        )
    })?;

    match format {
        ReportOutputFormat::Text => LevelView::status(&level),
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "level_status",
                "format_version": 1,
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "scope": { "account_id": account_id.to_string() },
                "data": {
                    "current_level": level.current_level,
                    "description": model::Level::level_description(level.current_level),
                    "risk_multiplier": output::decimal_string(level.risk_multiplier),
                    "status": level.status.to_string(),
                    "trades_at_level": level.trades_at_level,
                    "level_start_date": level.level_start_date.to_string()
                }
            });
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn level_history(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    use crate::views::LevelView;

    let account_id = resolve_level_account_id(trust, sub_matches, format)?;
    let days = sub_matches.get_one::<u32>("days").copied();
    let changes = trust
        .level_history_for_account(account_id, days)
        .map_err(|error| {
            output::report_error(
                format,
                "level_history_failed",
                format!("Unable to load level history: {error}"),
            )
        })?;

    match format {
        ReportOutputFormat::Text => LevelView::history(&changes),
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "level_history",
                "format_version": 1,
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "scope": { "account_id": account_id.to_string() },
                "filters": { "days": days },
                "data": changes.iter().map(|change| {
                    json!({
                        "id": change.id.to_string(),
                        "old_level": change.old_level,
                        "new_level": change.new_level,
                        "change_reason": change.change_reason,
                        "trigger_type": change.trigger_type.to_string(),
                        "changed_at": change.changed_at.to_string()
                    })
                }).collect::<Vec<_>>()
            });
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn level_change(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    use crate::views::LevelView;

    context::ensure_protected_keyword(trust, sub_matches, format, "manual level change")?;
    let account_id = resolve_level_account_id(trust, sub_matches, format)?;
    let target_level = *sub_matches.get_one::<u8>("to").ok_or_else(|| {
        output::report_error(
            format,
            "missing_level",
            "Missing required --to level argument",
        )
    })?;
    let reason = sub_matches
        .get_one::<String>("reason")
        .ok_or_else(|| output::report_error(format, "missing_reason", "Missing --reason"))?;
    let trigger = sub_matches
        .get_one::<String>("trigger")
        .ok_or_else(|| output::report_error(format, "missing_trigger", "Missing --trigger"))?;
    let parsed_trigger = LevelTrigger::from_str(trigger).map_err(|error| {
        output::report_error(
            format,
            "invalid_trigger",
            format!("Invalid --trigger value '{trigger}': {error}"),
        )
    })?;

    let (level, change) = trust
        .change_level(account_id, target_level, reason, parsed_trigger)
        .map_err(|error| {
            output::report_error(
                format,
                "level_change_failed",
                format!("Unable to apply level change: {error}"),
            )
        })?;

    match format {
        ReportOutputFormat::Text => {
            println!("Level change applied successfully.");
            LevelView::status(&level);
            println!();
            LevelView::history(&[change]);
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "level_change",
                "format_version": 1,
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "scope": { "account_id": account_id.to_string() },
                "data": {
                    "level": {
                        "current_level": level.current_level,
                        "description": model::Level::level_description(level.current_level),
                        "risk_multiplier": output::decimal_string(level.risk_multiplier),
                        "status": level.status.to_string(),
                        "level_start_date": level.level_start_date.to_string(),
                    },
                    "event": {
                        "old_level": change.old_level,
                        "new_level": change.new_level,
                        "change_reason": change.change_reason,
                        "trigger_type": change.trigger_type.to_string(),
                        "changed_at": change.changed_at.to_string(),
                    }
                }
            });
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn level_evaluate(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let account_id = resolve_level_account_id(trust, sub_matches, format)?;
    let snapshot = level_snapshot_from_args(sub_matches, format)?;
    let apply = sub_matches.get_flag("apply");
    if apply {
        context::ensure_protected_keyword(trust, sub_matches, format, "level policy apply")?;
    }

    let outcome = trust
        .evaluate_level_transition(account_id, snapshot, apply)
        .map_err(|error| {
            output::report_error(
                format,
                "level_evaluation_failed",
                format!("Unable to evaluate level transition: {error}"),
            )
        })?;

    match format {
        ReportOutputFormat::Text => level_output::print_level_evaluation_text(&outcome, apply),
        ReportOutputFormat::Json => {
            let payload = level_output::level_evaluation_payload(account_id, apply, &outcome);
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn level_progress(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let account_id = resolve_level_account_id(trust, sub_matches, format)?;
    let snapshot = level_snapshot_from_args(sub_matches, format)?;
    let outcome = trust
        .evaluate_level_transition(account_id, snapshot, false)
        .map_err(|error| {
            output::report_error(
                format,
                "level_progress_failed",
                format!("Unable to calculate level progress: {error}"),
            )
        })?;

    match format {
        ReportOutputFormat::Text => level_output::print_level_evaluation_text(&outcome, false),
        ReportOutputFormat::Json => {
            let payload = level_output::level_evaluation_payload(account_id, false, &outcome);
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct LevelCommand;
impl super::Command for LevelCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_level(trust, args)
    }
}
