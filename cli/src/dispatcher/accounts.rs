//! Legacy accounts command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_accounts_subcommand;
use crate::command_routing::AccountsSubcommand;
use crate::dialogs::AccountDialogBuilder;
use crate::dialogs::AccountSearchDialog;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output::ReportOutputFormat;
use clap::ArgMatches;
use core::TrustFacade;
use model::AccountType;
use model::BrokerKind;
use model::Currency;
use model::Environment;
use rust_decimal::Decimal;
use uuid::Uuid;

pub(in crate::dispatcher) fn has_non_interactive_account_args(sub_matches: &ArgMatches) -> bool {
    sub_matches.get_one::<String>("name").is_some()
        || sub_matches.get_one::<String>("description").is_some()
        || sub_matches.get_one::<String>("environment").is_some()
        || sub_matches.get_one::<String>("taxes").is_some()
        || sub_matches.get_one::<String>("earnings").is_some()
        || sub_matches.get_one::<String>("type").is_some()
        || sub_matches.get_one::<String>("broker").is_some()
        || sub_matches.get_one::<String>("broker-account-id").is_some()
        || sub_matches.get_one::<String>("parent").is_some()
}

pub(in crate::dispatcher) fn dispatch_accounts(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    match parse_accounts_subcommand(sub_matches) {
        AccountsSubcommand::Create(sub_sub_matches) => create_account(trust, sub_sub_matches)?,
        AccountsSubcommand::Search => search_account(trust),
        AccountsSubcommand::List(sub_sub_matches) => list_accounts(trust, sub_sub_matches)?,
        AccountsSubcommand::Balance(sub_sub_matches) => accounts_balance(trust, sub_sub_matches)?,
        AccountsSubcommand::Transfer(transfer_matches) => {
            transfer_accounts(trust, transfer_matches)?
        }
        AccountsSubcommand::Delete(delete_matches) => delete_account(trust, delete_matches)?,
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn create_account(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    context::ensure_protected_keyword(
        trust,
        sub_matches,
        ReportOutputFormat::Text,
        "account creation",
    )?;
    if has_non_interactive_account_args(sub_matches) {
        let name = sub_matches.get_one::<String>("name").cloned();
        let description = sub_matches.get_one::<String>("description").cloned();
        let environment = sub_matches.get_one::<String>("environment").cloned();
        let taxes = sub_matches.get_one::<String>("taxes").cloned();
        let earnings = sub_matches.get_one::<String>("earnings").cloned();
        let account_type = sub_matches.get_one::<String>("type").cloned();
        let broker = sub_matches.get_one::<String>("broker").cloned();
        let broker_account_id = sub_matches.get_one::<String>("broker-account-id").cloned();
        let parent = sub_matches.get_one::<String>("parent").cloned();

        let name = name.ok_or_else(|| CliError::new("missing_name", "Missing --name"))?;
        let description = description
            .ok_or_else(|| CliError::new("missing_description", "Missing --description"))?;
        let environment = environment
            .ok_or_else(|| CliError::new("missing_environment", "Missing --environment"))?;
        let taxes = taxes.ok_or_else(|| CliError::new("missing_taxes", "Missing --taxes"))?;
        let earnings =
            earnings.ok_or_else(|| CliError::new("missing_earnings", "Missing --earnings"))?;

        let environment = parse_environment(&environment)?;
        let taxes_percentage = Decimal::from_str_exact(&taxes)
            .map_err(|_| CliError::new("invalid_decimal", "Invalid --taxes decimal"))?
            .checked_div(Decimal::new(100, 0))
            .ok_or_else(|| CliError::new("invalid_decimal", "Invalid --taxes percentage"))?;
        let earnings_percentage = Decimal::from_str_exact(&earnings)
            .map_err(|_| CliError::new("invalid_decimal", "Invalid --earnings decimal"))?
            .checked_div(Decimal::new(100, 0))
            .ok_or_else(|| CliError::new("invalid_decimal", "Invalid --earnings percentage"))?;

        let account_type = match account_type {
            Some(v) => parse_account_type(&v)?,
            None => AccountType::Primary,
        };
        let broker_kind = match broker {
            Some(v) => parse_broker_kind(&v)?,
            None => BrokerKind::Alpaca,
        };
        let parent_account_id = match parent {
            Some(v) => Some(
                Uuid::parse_str(&v)
                    .map_err(|_| CliError::new("invalid_uuid", "Invalid --parent UUID"))?,
            ),
            None => None,
        };
        let broker_account_id = broker_account_id.filter(|value| !value.trim().is_empty());

        if broker_kind == BrokerKind::Ibkr
            && account_type == AccountType::Primary
            && broker_account_id.is_none()
        {
            return Err(CliError::new(
                "missing_broker_account_id",
                "Missing --broker-account-id for an IBKR primary account",
            ));
        }

        let account = trust
            .create_account_with_profile(
                &name,
                &description,
                environment,
                taxes_percentage,
                earnings_percentage,
                account_type,
                parent_account_id,
                broker_kind,
                broker_account_id.as_deref(),
            )
            .map_err(|e| CliError::new("create_account_failed", e.to_string()))?;
        println!("Account created:");
        println!("  id: {}", account.id);
        println!("  name: {}", account.name);
        println!("  type: {}", account.account_type);
        println!("  broker: {}", account.broker_kind);
        println!(
            "  broker_account_id: {}",
            account.broker_account_id.as_deref().unwrap_or("none")
        );
        println!(
            "  parent: {}",
            account
                .parent_account_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string())
        );
    } else {
        AccountDialogBuilder::new()
            .name()
            .description()
            .environment()
            .tax_percentage()
            .earnings_percentage()
            .build(trust)
            .display();
    }
    Ok(())
}

pub(in crate::dispatcher) fn search_account(trust: &mut TrustFacade) {
    AccountSearchDialog::new().search(trust).display(trust);
}

pub(in crate::dispatcher) fn list_accounts(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
) -> Result<(), CliError> {
    let hierarchy = matches.get_flag("hierarchy");
    let accounts = trust
        .search_all_accounts()
        .map_err(|e| CliError::new("accounts_list_failed", e.to_string()))?;

    if accounts.is_empty() {
        println!("No accounts found.");
        return Ok(());
    }

    if !hierarchy {
        for account in accounts {
            println!("{} {} {}", account.id, account.name, account.account_type);
        }
        return Ok(());
    }

    let mut roots: Vec<_> = accounts
        .iter()
        .filter(|a| a.parent_account_id.is_none())
        .cloned()
        .collect();
    roots.sort_by(|a, b| a.name.cmp(&b.name));

    for root in roots {
        println!("{} ({}) [{}]", root.name, root.id, root.account_type);
        let mut children: Vec<_> = accounts
            .iter()
            .filter(|a| a.parent_account_id == Some(root.id))
            .cloned()
            .collect();
        children.sort_by(|a, b| a.name.cmp(&b.name));
        for child in children {
            println!("  - {} ({}) [{}]", child.name, child.id, child.account_type);
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn accounts_balance(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
) -> Result<(), CliError> {
    let detailed = matches.get_flag("detailed");
    let accounts = trust
        .search_all_accounts()
        .map_err(|e| CliError::new("accounts_balance_failed", e.to_string()))?;
    if accounts.is_empty() {
        println!("No accounts found.");
        return Ok(());
    }

    for account in accounts {
        let balances = trust
            .search_all_balances(account.id)
            .map_err(|e| CliError::new("accounts_balance_failed", e.to_string()))?;

        if detailed {
            println!(
                "{} ({}) [{}]",
                account.name, account.id, account.account_type
            );
            for b in balances {
                let basis = b.total_balance;
                println!(
                    "  {} total={} available={} in_trade={} taxed={} earnings={}",
                    b.currency,
                    crate::zen::amount_share(b.total_balance, basis),
                    crate::zen::amount_share(b.total_available, basis),
                    crate::zen::amount_share(b.total_in_trade, basis),
                    crate::zen::amount_share(b.taxed, basis),
                    crate::zen::amount_share(b.total_earnings, basis)
                );
            }
        } else {
            let total = balances.into_iter().fold(Decimal::ZERO, |acc, b| {
                acc.checked_add(b.total_balance).unwrap_or(acc)
            });
            println!(
                "{} ({}) total={}",
                account.name,
                account.id,
                crate::zen::amount_share(total, total)
            );
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn delete_account(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
) -> Result<(), CliError> {
    context::ensure_protected_keyword(
        trust,
        matches,
        ReportOutputFormat::Text,
        "account deletion",
    )?;
    let account_id = matches
        .get_one::<String>("account")
        .ok_or_else(|| CliError::new("missing_account", "Missing --account"))?;
    let account_id = Uuid::parse_str(account_id)
        .map_err(|_| CliError::new("invalid_uuid", "Invalid --account UUID"))?;
    let force = matches.get_flag("force");

    let account = trust
        .delete_account(account_id, force)
        .map_err(|e| CliError::new("delete_account_failed", e.to_string()))?;

    println!("Account deleted:");
    println!("  id: {}", account.id);
    println!("  name: {}", account.name);
    println!("  force: {}", force);
    Ok(())
}

pub(in crate::dispatcher) fn parse_environment(raw: &str) -> Result<Environment, CliError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "paper" | "sandbox" => Ok(Environment::Paper),
        "live" | "production" => Ok(Environment::Live),
        _ => Err(CliError::new(
            "invalid_environment",
            "Invalid --environment value (expected paper|live)",
        )),
    }
}

pub(in crate::dispatcher) fn parse_account_type(raw: &str) -> Result<AccountType, CliError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "primary" => Ok(AccountType::Primary),
        "earnings" => Ok(AccountType::Earnings),
        "tax-reserve" | "tax_reserve" => Ok(AccountType::TaxReserve),
        "reinvestment" => Ok(AccountType::Reinvestment),
        "insurance" => Ok(AccountType::Insurance),
        _ => Err(CliError::new(
            "invalid_account_type",
            "Invalid --type value (expected primary|earnings|tax-reserve|reinvestment|insurance)",
        )),
    }
}

pub(in crate::dispatcher) fn parse_broker_kind(raw: &str) -> Result<BrokerKind, CliError> {
    raw.parse::<BrokerKind>().map_err(|_| {
        CliError::new(
            "invalid_broker_kind",
            "Invalid --broker value (expected alpaca|ibkr)",
        )
    })
}

pub(in crate::dispatcher) fn parse_trading_vehicle_category(
    raw: &str,
) -> Result<model::TradingVehicleCategory, CliError> {
    raw.parse::<model::TradingVehicleCategory>().map_err(|_| {
        CliError::new(
            "invalid_trading_vehicle_category",
            "Invalid --category value (expected stock|etf|bond|crypto|fiat)",
        )
    })
}

pub(in crate::dispatcher) fn transfer_accounts(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
) -> Result<(), CliError> {
    let from_id_str = crate::dispatcher::args::required_string_arg(matches, "from")?;
    let to_id_str = crate::dispatcher::args::required_string_arg(matches, "to")?;
    let amount_str = crate::dispatcher::args::required_string_arg(matches, "amount")?;
    let reason = crate::dispatcher::args::required_string_arg(matches, "reason")?;

    let from_id = Uuid::parse_str(from_id_str)
        .map_err(|_| CliError::new("invalid_uuid", "Invalid --from UUID"))?;
    let to_id = Uuid::parse_str(to_id_str)
        .map_err(|_| CliError::new("invalid_uuid", "Invalid --to UUID"))?;
    let amount = Decimal::from_str_exact(amount_str)
        .map_err(|_| CliError::new("invalid_decimal", "Invalid --amount decimal"))?;

    context::ensure_protected_keyword(
        trust,
        matches,
        ReportOutputFormat::Text,
        "account transfer",
    )?;
    let currency = Currency::USD;
    let (withdrawal_id, deposit_id) = trust
        .transfer_between_accounts(from_id, to_id, amount, currency, reason)
        .map_err(|e| CliError::new("transfer_failed", e.to_string()))?;

    println!("Transfer completed:");
    println!("  amount: {}", crate::zen::amount(amount));
    println!("  from:   {from_id}");
    println!("  to:     {to_id}");
    println!("  reason: {reason}");
    println!("  withdrawal_tx_id: {withdrawal_id}");
    println!("  deposit_tx_id:    {deposit_id}");
    Ok(())
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct AccountsCommand;
impl super::Command for AccountsCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_accounts(trust, args)
    }
}
