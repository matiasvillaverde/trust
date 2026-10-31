//! Legacy advisor config command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_advisor_subcommand;
use crate::command_routing::AdvisorSubcommand;
use crate::dispatcher::args;
use crate::dispatcher::args::bool_arg_present;
use crate::dispatcher::args::string_arg_present;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output::ReportOutputFormat;
use advisor::AdvisorConfig;
use advisor::AdvisorConfigUpdate;
use advisor::CalendarProvider;
use clap::ArgMatches;
use core::services::AdvisoryThresholds;
use core::services::TradeProposal;
use core::TrustFacade;
use rust_decimal::Decimal;
use std::str::FromStr;
use uuid::Uuid;

pub(in crate::dispatcher) fn dispatch_advisor(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    match parse_advisor_subcommand(sub_matches) {
        AdvisorSubcommand::Configure(sub_sub_matches) => advisor_configure(trust, sub_sub_matches)?,
        AdvisorSubcommand::Check(sub_sub_matches) => advisor_check(trust, sub_sub_matches)?,
        AdvisorSubcommand::Status(sub_sub_matches) => advisor_status(trust, sub_sub_matches)?,
        AdvisorSubcommand::History(sub_sub_matches) => advisor_history(trust, sub_sub_matches)?,
    }
    Ok(())
}

pub(in crate::dispatcher) fn advisor_configure(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let provider_mode = advisor_provider_config_requested(sub_matches);
    let threshold_mode = advisor_threshold_config_requested(sub_matches);
    if provider_mode && threshold_mode {
        return Err(CliError::new(
            "advisor_configure_mode_conflict",
            "Advisor provider keys and concentration thresholds must be configured separately",
        ));
    }
    if provider_mode {
        return advisor_provider_configure(sub_matches);
    }
    advisor_thresholds_configure(trust, sub_matches)
}

pub(in crate::dispatcher) fn advisor_thresholds_configure(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let account_id = args::parse_uuid_arg(sub_matches, "account", ReportOutputFormat::Text)?;
    let sector_limit =
        args::parse_decimal_arg(sub_matches, "sector-limit", ReportOutputFormat::Text)?;
    let asset_class_limit =
        args::parse_decimal_arg(sub_matches, "asset-class-limit", ReportOutputFormat::Text)?;
    let single_position_limit = args::parse_decimal_arg(
        sub_matches,
        "single-position-limit",
        ReportOutputFormat::Text,
    )?;
    context::ensure_protected_keyword(
        trust,
        sub_matches,
        ReportOutputFormat::Text,
        "advisor configure",
    )?;

    trust
        .configure_advisory_thresholds(
            account_id,
            AdvisoryThresholds {
                sector_limit_pct: sector_limit,
                asset_class_limit_pct: asset_class_limit,
                single_position_limit_pct: single_position_limit,
            },
        )
        .map_err(|e| CliError::new("advisor_configure_failed", e.to_string()))?;
    println!("Advisor thresholds configured for account {account_id}");
    Ok(())
}

pub(in crate::dispatcher) fn advisor_provider_configure(
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let update = advisor_config_update_from_matches(sub_matches)?;
    if !update.is_empty() {
        update
            .apply()
            .map_err(|e| CliError::new("advisor_configure_failed", e.to_string()))?;
    }
    if sub_matches.get_flag("show") || update.is_empty() {
        let config = AdvisorConfig::read()
            .map_err(|e| CliError::new("advisor_configure_failed", e.to_string()))?;
        print_advisor_provider_config(&config);
    } else {
        let config = AdvisorConfig::read()
            .map_err(|e| CliError::new("advisor_configure_failed", e.to_string()))?;
        println!("Advisor provider configuration updated.");
        print_advisor_provider_config(&config);
    }
    Ok(())
}

pub(in crate::dispatcher) fn advisor_check(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let account_id = Uuid::parse_str(crate::dispatcher::args::required_string_arg(
        sub_matches,
        "account",
    )?)
    .map_err(|_| CliError::new("invalid_uuid", "Invalid --account UUID"))?;
    let symbol = crate::dispatcher::args::required_string_arg(sub_matches, "symbol")?.to_string();
    let entry_price = Decimal::from_str_exact(crate::dispatcher::args::required_string_arg(
        sub_matches,
        "entry",
    )?)
    .map_err(|_| CliError::new("invalid_decimal", "Invalid --entry"))?;
    let quantity = Decimal::from_str_exact(crate::dispatcher::args::required_string_arg(
        sub_matches,
        "quantity",
    )?)
    .map_err(|_| CliError::new("invalid_decimal", "Invalid --quantity"))?;
    let sector = sub_matches.get_one::<String>("sector").cloned();
    let asset_class = sub_matches.get_one::<String>("asset-class").cloned();

    let result = trust
        .advisory_check_trade(TradeProposal {
            account_id,
            symbol,
            sector,
            asset_class,
            entry_price,
            quantity,
        })
        .map_err(|e| CliError::new("advisor_check_failed", e.to_string()))?;

    println!("Advisory level: {:?}", result.level);
    println!(
        "Projected concentrations: sector={} asset_class={} single_position={}",
        result.projected_sector_pct,
        result.projected_asset_class_pct,
        result.projected_single_position_pct
    );
    if !result.warnings.is_empty() {
        println!("Warnings:");
        for warning in result.warnings {
            println!("  - {warning}");
        }
    }
    if !result.recommendations.is_empty() {
        println!("Recommendations:");
        for recommendation in result.recommendations {
            println!("  - {recommendation}");
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn advisor_status(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let account_id = Uuid::parse_str(crate::dispatcher::args::required_string_arg(
        sub_matches,
        "account",
    )?)
    .map_err(|_| CliError::new("invalid_uuid", "Invalid --account UUID"))?;
    let status = trust
        .advisory_status_for_account(account_id)
        .map_err(|e| CliError::new("advisor_status_failed", e.to_string()))?;
    println!("Portfolio advisory level: {:?}", status.level);
    println!("Top sector concentration: {}", status.top_sector_pct);
    println!(
        "Top asset class concentration: {}",
        status.top_asset_class_pct
    );
    println!(
        "Top single position concentration: {}",
        status.top_position_pct
    );
    for warning in status.warnings {
        println!("  - {warning}");
    }
    Ok(())
}

pub(in crate::dispatcher) fn advisor_history(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let account_id = Uuid::parse_str(crate::dispatcher::args::required_string_arg(
        sub_matches,
        "account",
    )?)
    .map_err(|_| CliError::new("invalid_uuid", "Invalid --account UUID"))?;
    let days = sub_matches
        .get_one::<String>("days")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(30);
    let entries = trust.advisory_history_for_account(account_id, days);
    if entries.is_empty() {
        println!("No advisory history entries.");
        return Ok(());
    }
    for entry in entries {
        println!(
            "{} {:?} {} {}",
            entry.created_at, entry.level, entry.symbol, entry.summary
        );
    }
    Ok(())
}

pub(in crate::dispatcher) fn advisor_provider_config_requested(matches: &ArgMatches) -> bool {
    bool_arg_present(matches, "show")
        || string_arg_present(matches, "calendar-provider")
        || string_arg_present(matches, "calendar-api-key")
        || string_arg_present(matches, "claude-api-key")
}

pub(in crate::dispatcher) fn advisor_threshold_config_requested(matches: &ArgMatches) -> bool {
    string_arg_present(matches, "account")
        || string_arg_present(matches, "sector-limit")
        || string_arg_present(matches, "asset-class-limit")
        || string_arg_present(matches, "single-position-limit")
        || string_arg_present(matches, "confirm-protected")
}

pub(in crate::dispatcher) fn advisor_config_update_from_matches(
    matches: &ArgMatches,
) -> Result<AdvisorConfigUpdate, CliError> {
    let calendar_provider = matches
        .get_one::<String>("calendar-provider")
        .map(|value| {
            CalendarProvider::from_str(value)
                .map_err(|e| CliError::new("invalid_calendar_provider", e.to_string()))
        })
        .transpose()?;

    Ok(AdvisorConfigUpdate {
        calendar_provider,
        calendar_api_key: matches.get_one::<String>("calendar-api-key").cloned(),
        claude_api_key: matches.get_one::<String>("claude-api-key").cloned(),
    })
}

pub(in crate::dispatcher) fn print_advisor_provider_config(config: &AdvisorConfig) {
    println!("Advisor configuration:");
    println!("Calendar provider: {}", config.calendar_provider);
    println!("Calendar API key: {}", config.calendar_api_key_display());
    println!("Claude API key: {}", config.claude_api_key_display());
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct AdvisorCommand;
impl super::Command for AdvisorCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_advisor(trust, args)
    }
}
