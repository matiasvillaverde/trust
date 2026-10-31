//! Legacy level policy command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::levels;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use clap::ArgMatches;
use core::TrustFacade;
use model::LevelAdjustmentRules;
use rust_decimal::Decimal;
use serde_json::json;
use serde_json::Value;
use std::str::FromStr;

pub(in crate::dispatcher) fn level_rules_show(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let account_id = levels::resolve_level_account_id(trust, sub_matches, format)?;
    let rules = trust
        .level_adjustment_rules_for_account(account_id)
        .map_err(|error| {
            output::report_error(
                format,
                "level_rules_show_failed",
                format!("Unable to load level rules: {error}"),
            )
        })?;

    match format {
        ReportOutputFormat::Text => {
            println!("Level Adjustment Rules");
            println!("======================");
            println!(
                "monthly_loss_downgrade_pct: {}",
                rules.monthly_loss_downgrade_pct
            );
            println!(
                "single_loss_downgrade_pct: {}",
                rules.single_loss_downgrade_pct
            );
            println!(
                "upgrade_profitable_trades: {}",
                rules.upgrade_profitable_trades
            );
            println!("upgrade_win_rate_pct: {}", rules.upgrade_win_rate_pct);
            println!(
                "upgrade_consecutive_wins: {}",
                rules.upgrade_consecutive_wins
            );
            println!(
                "cooldown_profitable_trades: {}",
                rules.cooldown_profitable_trades
            );
            println!("cooldown_win_rate_pct: {}", rules.cooldown_win_rate_pct);
            println!(
                "cooldown_consecutive_wins: {}",
                rules.cooldown_consecutive_wins
            );
            println!(
                "recovery_profitable_trades: {}",
                rules.recovery_profitable_trades
            );
            println!("recovery_win_rate_pct: {}", rules.recovery_win_rate_pct);
            println!(
                "recovery_consecutive_wins: {}",
                rules.recovery_consecutive_wins
            );
            println!(
                "min_trades_at_level_for_upgrade: {}",
                rules.min_trades_at_level_for_upgrade
            );
            println!("max_changes_in_30_days: {}", rules.max_changes_in_30_days);
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "level_rules",
                "format_version": 1,
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "scope": { "account_id": account_id.to_string() },
                "data": level_rules_payload(&rules),
            });
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn level_rules_set(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    context::ensure_protected_keyword(trust, sub_matches, format, "level rules update")?;
    let account_id = levels::resolve_level_account_id(trust, sub_matches, format)?;
    let rule_key = sub_matches
        .get_one::<String>("rule")
        .ok_or_else(|| output::report_error(format, "missing_rule_key", "Missing --rule"))?;
    let value = sub_matches
        .get_one::<String>("value")
        .ok_or_else(|| output::report_error(format, "missing_rule_value", "Missing --value"))?;

    let mut rules = trust
        .level_adjustment_rules_for_account(account_id)
        .map_err(|error| {
            output::report_error(
                format,
                "level_rules_read_failed",
                format!("Unable to read current level rules: {error}"),
            )
        })?;

    apply_level_rule_update(&mut rules, rule_key, value, format)?;
    let updated = trust
        .set_level_adjustment_rules(account_id, &rules)
        .map_err(|error| {
            output::report_error(
                format,
                "level_rules_set_failed",
                format!("Unable to update level rules: {error}"),
            )
        })?;

    match format {
        ReportOutputFormat::Text => {
            println!("Level rule updated: {}={}", rule_key, value);
            println!();
            level_rules_show(trust, sub_matches, format)?;
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "level_rules_set",
                "format_version": 1,
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "scope": { "account_id": account_id.to_string() },
                "data": {
                    "updated_key": rule_key,
                    "updated_value": value,
                    "rules": level_rules_payload(&updated),
                }
            });
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn apply_level_rule_update(
    rules: &mut LevelAdjustmentRules,
    key: &str,
    value: &str,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let parse_decimal = || {
        Decimal::from_str(value).map_err(|error| {
            output::report_error(
                format,
                "invalid_rule_value",
                format!("Invalid decimal value '{value}': {error}"),
            )
        })
    };
    let parse_u32 = || {
        value.parse::<u32>().map_err(|error| {
            output::report_error(
                format,
                "invalid_rule_value",
                format!("Invalid integer value '{value}': {error}"),
            )
        })
    };

    match key {
        "monthly_loss_downgrade_pct" => rules.monthly_loss_downgrade_pct = parse_decimal()?,
        "single_loss_downgrade_pct" => rules.single_loss_downgrade_pct = parse_decimal()?,
        "upgrade_profitable_trades" => rules.upgrade_profitable_trades = parse_u32()?,
        "upgrade_win_rate_pct" => rules.upgrade_win_rate_pct = parse_decimal()?,
        "upgrade_consecutive_wins" => rules.upgrade_consecutive_wins = parse_u32()?,
        "cooldown_profitable_trades" => rules.cooldown_profitable_trades = parse_u32()?,
        "cooldown_win_rate_pct" => rules.cooldown_win_rate_pct = parse_decimal()?,
        "cooldown_consecutive_wins" => rules.cooldown_consecutive_wins = parse_u32()?,
        "recovery_profitable_trades" => rules.recovery_profitable_trades = parse_u32()?,
        "recovery_win_rate_pct" => rules.recovery_win_rate_pct = parse_decimal()?,
        "recovery_consecutive_wins" => rules.recovery_consecutive_wins = parse_u32()?,
        "min_trades_at_level_for_upgrade" => rules.min_trades_at_level_for_upgrade = parse_u32()?,
        "max_changes_in_30_days" => rules.max_changes_in_30_days = parse_u32()?,
        _ => {
            return Err(output::report_error(
                format,
                "invalid_rule_key",
                format!("Unsupported --rule key '{key}'"),
            ));
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn level_rules_payload(rules: &LevelAdjustmentRules) -> Value {
    json!({
        "monthly_loss_downgrade_pct": output::decimal_string(rules.monthly_loss_downgrade_pct),
        "single_loss_downgrade_pct": output::decimal_string(rules.single_loss_downgrade_pct),
        "upgrade_profitable_trades": rules.upgrade_profitable_trades,
        "upgrade_win_rate_pct": output::decimal_string(rules.upgrade_win_rate_pct),
        "upgrade_consecutive_wins": rules.upgrade_consecutive_wins,
        "cooldown_profitable_trades": rules.cooldown_profitable_trades,
        "cooldown_win_rate_pct": output::decimal_string(rules.cooldown_win_rate_pct),
        "cooldown_consecutive_wins": rules.cooldown_consecutive_wins,
        "recovery_profitable_trades": rules.recovery_profitable_trades,
        "recovery_win_rate_pct": output::decimal_string(rules.recovery_win_rate_pct),
        "recovery_consecutive_wins": rules.recovery_consecutive_wins,
        "min_trades_at_level_for_upgrade": rules.min_trades_at_level_for_upgrade,
        "max_changes_in_30_days": rules.max_changes_in_30_days,
    })
}
