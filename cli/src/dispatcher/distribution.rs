//! Legacy distribution command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_distribution_subcommand;
use crate::command_routing::DistributionSubcommand;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output::ReportOutputFormat;
use clap::ArgMatches;
use core::TrustFacade;
use dialoguer::Password;
use model::AccountType;
use model::Currency;
use rust_decimal::Decimal;
use uuid::Uuid;

pub(in crate::dispatcher) fn dispatch_distribution(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    match parse_distribution_subcommand(sub_matches) {
        DistributionSubcommand::Configure(configure_matches) => {
            context::ensure_protected_keyword(
                trust,
                configure_matches,
                ReportOutputFormat::Text,
                "distribution configuration",
            )?;
            configure_distribution(trust, configure_matches)?
        }
        DistributionSubcommand::Execute(execute_matches) => {
            context::ensure_protected_keyword(
                trust,
                execute_matches,
                ReportOutputFormat::Text,
                "distribution execution",
            )?;
            execute_distribution(trust, execute_matches)?
        }
        DistributionSubcommand::History(history_matches) => {
            distribution_history(trust, history_matches)?
        }
        DistributionSubcommand::Insurance(insurance_matches) => {
            distribution_insurance_status(trust, insurance_matches)?
        }
        DistributionSubcommand::Rules(rules_matches) => {
            if let Some(("show", nested)) = rules_matches.subcommand() {
                distribution_rules(trust, nested)?
            } else {
                distribution_rules(trust, rules_matches)?
            }
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn configure_distribution(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
) -> Result<(), CliError> {
    let account_id_str = crate::dispatcher::args::required_string_arg(matches, "account-id")?;
    let earnings_str = crate::dispatcher::args::required_string_arg(matches, "earnings")?;
    let tax_str = crate::dispatcher::args::required_string_arg(matches, "tax")?;
    let reinvestment_str = crate::dispatcher::args::required_string_arg(matches, "reinvestment")?;
    let insurance_str = matches
        .get_one::<String>("insurance")
        .map(String::as_str)
        .unwrap_or("0");
    let threshold_str = crate::dispatcher::args::required_string_arg(matches, "threshold")?;
    let password = if let Some(p) = matches.get_one::<String>("password") {
        p.to_string()
    } else {
        Password::new()
            .with_prompt("Distribution configuration password")
            .with_confirmation("Confirm password", "Passwords do not match")
            .interact()
            .map_err(|e| CliError::new("password_prompt_failed", e.to_string()))?
    };

    let account_id = Uuid::parse_str(account_id_str)
        .map_err(|_| CliError::new("invalid_uuid", "Invalid --account-id UUID"))?;

    let earnings_pct = Decimal::from_str_exact(earnings_str)
        .map_err(|_| CliError::new("invalid_decimal", "Invalid --earnings decimal"))?
        .checked_div(Decimal::new(100, 0))
        .ok_or_else(|| CliError::new("invalid_decimal", "Invalid --earnings percentage"))?;
    let tax_pct = Decimal::from_str_exact(tax_str)
        .map_err(|_| CliError::new("invalid_decimal", "Invalid --tax decimal"))?
        .checked_div(Decimal::new(100, 0))
        .ok_or_else(|| CliError::new("invalid_decimal", "Invalid --tax percentage"))?;
    let reinvestment_pct = Decimal::from_str_exact(reinvestment_str)
        .map_err(|_| CliError::new("invalid_decimal", "Invalid --reinvestment decimal"))?
        .checked_div(Decimal::new(100, 0))
        .ok_or_else(|| CliError::new("invalid_decimal", "Invalid --reinvestment percentage"))?;
    let insurance_pct = Decimal::from_str_exact(insurance_str)
        .map_err(|_| CliError::new("invalid_decimal", "Invalid --insurance decimal"))?
        .checked_div(Decimal::new(100, 0))
        .ok_or_else(|| CliError::new("invalid_decimal", "Invalid --insurance percentage"))?;
    let threshold = Decimal::from_str_exact(threshold_str)
        .map_err(|_| CliError::new("invalid_decimal", "Invalid --threshold decimal"))?;

    trust
        .configure_distribution_with_insurance(
            account_id,
            earnings_pct,
            tax_pct,
            reinvestment_pct,
            insurance_pct,
            threshold,
            &password,
        )
        .map_err(|e| CliError::new("configure_distribution_failed", e.to_string()))?;

    println!("Distribution rules configured:");
    println!("  account_id: {account_id}");
    println!("  earnings: {earnings_pct}");
    println!("  tax: {tax_pct}");
    println!("  reinvestment: {reinvestment_pct}");
    println!("  insurance: {insurance_pct}");
    println!("  minimum_threshold: {}", crate::zen::amount(threshold));
    Ok(())
}

pub(in crate::dispatcher) fn execute_distribution(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
) -> Result<(), CliError> {
    let account_id_str = crate::dispatcher::args::required_string_arg(matches, "account-id")?;
    let amount_str = crate::dispatcher::args::required_string_arg(matches, "amount")?;

    let account_id = Uuid::parse_str(account_id_str)
        .map_err(|_| CliError::new("invalid_uuid", "Invalid --account-id UUID"))?;
    let amount = Decimal::from_str_exact(amount_str)
        .map_err(|_| CliError::new("invalid_decimal", "Invalid --amount decimal"))?;

    let currency = Currency::USD;
    let result = trust
        .execute_distribution(account_id, amount, currency)
        .map_err(|e| CliError::new("execute_distribution_failed", e.to_string()))?;

    println!("Distribution executed:");
    println!("  source_account_id: {}", result.source_account_id);
    println!(
        "  original_amount: {}",
        crate::zen::amount_share(result.original_amount, result.original_amount)
    );
    println!(
        "  earnings_amount: {}",
        crate::zen::amount_share(
            result.earnings_amount.unwrap_or_default(),
            result.original_amount,
        )
    );
    println!(
        "  tax_amount: {}",
        crate::zen::amount_share(
            result.tax_amount.unwrap_or_default(),
            result.original_amount
        )
    );
    println!(
        "  reinvestment_amount: {}",
        crate::zen::amount_share(
            result.reinvestment_amount.unwrap_or_default(),
            result.original_amount,
        )
    );
    println!(
        "  insurance_amount: {}",
        crate::zen::amount_share(
            result.insurance_amount.unwrap_or_default(),
            result.original_amount,
        )
    );
    println!(
        "  transactions_created: {}",
        result.transactions_created.len()
    );
    Ok(())
}

pub(in crate::dispatcher) fn distribution_history(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
) -> Result<(), CliError> {
    let account_id_str = crate::dispatcher::args::required_string_arg(matches, "account-id")?;
    let limit = matches
        .get_one::<String>("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(20);

    let account_id = Uuid::parse_str(account_id_str)
        .map_err(|_| CliError::new("invalid_uuid", "Invalid --account-id UUID"))?;

    let mut entries = trust
        .distribution_history(account_id)
        .map_err(|e| CliError::new("distribution_history_failed", e.to_string()))?;

    if entries.len() > limit {
        entries.truncate(limit);
    }

    println!("Distribution History (most recent first):");
    for entry in entries {
        let basis = entry.original_amount;
        println!(
            "- at={} trade_id={} amount={} earnings={} tax={} reinvestment={} insurance={}",
            entry.distribution_date,
            entry
                .trade_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            crate::zen::amount_share(entry.original_amount, basis),
            entry
                .earnings_amount
                .map(|v| crate::zen::amount_share(v, basis))
                .unwrap_or_else(|| crate::zen::amount_share(Decimal::ZERO, basis)),
            entry
                .tax_amount
                .map(|v| crate::zen::amount_share(v, basis))
                .unwrap_or_else(|| crate::zen::amount_share(Decimal::ZERO, basis)),
            entry
                .reinvestment_amount
                .map(|v| crate::zen::amount_share(v, basis))
                .unwrap_or_else(|| crate::zen::amount_share(Decimal::ZERO, basis)),
            entry
                .insurance_amount
                .map(|v| crate::zen::amount_share(v, basis))
                .unwrap_or_else(|| crate::zen::amount_share(Decimal::ZERO, basis)),
        );
    }

    Ok(())
}

pub(in crate::dispatcher) fn distribution_insurance_status(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
) -> Result<(), CliError> {
    let account_id_str = crate::dispatcher::args::required_string_arg(matches, "account-id")?;
    let source_account_id = Uuid::parse_str(account_id_str)
        .map_err(|_| CliError::new("invalid_uuid", "Invalid --account-id UUID"))?;

    let accounts = trust
        .search_all_accounts()
        .map_err(|e| CliError::new("insurance_status_failed", e.to_string()))?;
    let insurance_account = accounts
        .iter()
        .find(|account| {
            account.parent_account_id == Some(source_account_id)
                && account.account_type == AccountType::Insurance
        })
        .ok_or_else(|| {
            CliError::new(
                "insurance_account_not_found",
                "Missing insurance subaccount for distribution",
            )
        })?;

    let transactions = trust
        .get_account_transactions(insurance_account.id)
        .map_err(|e| CliError::new("insurance_status_failed", e.to_string()))?;
    let balance = transactions.iter().try_fold(Decimal::ZERO, |total, tx| {
        total
            .checked_add(tx.amount)
            .ok_or_else(|| CliError::new("insurance_status_failed", "Balance overflow"))
    })?;
    let history = trust
        .distribution_history(source_account_id)
        .map_err(|e| CliError::new("insurance_status_failed", e.to_string()))?;
    let allocated = history.iter().try_fold(Decimal::ZERO, |total, entry| {
        total
            .checked_add(entry.insurance_amount.unwrap_or_default())
            .ok_or_else(|| CliError::new("insurance_status_failed", "Allocation overflow"))
    })?;
    let distribution_count = history
        .iter()
        .filter(|entry| entry.insurance_amount.unwrap_or_default() > Decimal::ZERO)
        .count();

    println!("Insurance reserve:");
    println!("  source_account_id: {source_account_id}");
    println!("  insurance_account_id: {}", insurance_account.id);
    println!("  insurance_account_name: {}", insurance_account.name);
    println!("  balance: {}", crate::zen::amount(balance));
    println!("  allocated_total: {}", crate::zen::amount(allocated));
    println!("  distribution_count: {distribution_count}");
    Ok(())
}

pub(in crate::dispatcher) fn distribution_rules(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
) -> Result<(), CliError> {
    let account_id_str = crate::dispatcher::args::required_string_arg(matches, "account-id")?;
    let account_id = Uuid::parse_str(account_id_str)
        .map_err(|_| CliError::new("invalid_uuid", "Invalid --account-id UUID"))?;

    let rules = trust
        .distribution_rules_for_account(account_id)
        .map_err(|e| CliError::new("distribution_rules_failed", e.to_string()))?;

    println!("Distribution rules:");
    println!("  account_id: {}", rules.account_id);
    println!("  earnings_percent: {}", rules.earnings_percent);
    println!("  tax_percent: {}", rules.tax_percent);
    println!("  reinvestment_percent: {}", rules.reinvestment_percent);
    println!("  insurance_percent: {}", rules.insurance_percent);
    println!(
        "  minimum_threshold: {}",
        crate::zen::amount(rules.minimum_threshold)
    );
    Ok(())
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct DistributionCommand;
impl super::Command for DistributionCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_distribution(trust, args)
    }
}
