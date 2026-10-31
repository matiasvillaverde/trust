//! Legacy trade commands command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_trade_subcommand;
use crate::command_routing::TradeSubcommand;
use crate::dialogs::CancelDialogBuilder;
use crate::dialogs::CloseDialogBuilder;
use crate::dialogs::ExitDialogBuilder;
use crate::dialogs::FillTradeDialogBuilder;
use crate::dialogs::FundingDialogBuilder;
use crate::dialogs::ModifyDialogBuilder;
use crate::dialogs::SubmitDialogBuilder;
use crate::dialogs::SyncTradeDialogBuilder;
use crate::dialogs::TradeDialogBuilder;
use crate::dispatcher::args;
use crate::dispatcher::autopsy;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use crate::dispatcher::sizing;
use crate::dispatcher::trade_advisor;
use crate::dispatcher::trade_queries;
use clap::ArgMatches;
use core::TrustFacade;
use model::Currency;
use model::DraftTrade;
use model::OrderCategory;
use model::TradeCategory;
use rust_decimal::Decimal;
use std::str::FromStr;

pub(in crate::dispatcher) fn has_non_interactive_trade_args(sub_matches: &ArgMatches) -> bool {
    sub_matches.get_one::<String>("account").is_some()
        || sub_matches.get_one::<String>("symbol").is_some()
        || sub_matches.get_one::<String>("category").is_some()
        || sub_matches.get_one::<String>("entry").is_some()
        || sub_matches.get_one::<String>("stop").is_some()
        || sub_matches.get_one::<String>("safety-order-type").is_some()
        || sub_matches.get_one::<String>("target").is_some()
        || sub_matches.get_one::<String>("quantity").is_some()
}

pub(in crate::dispatcher) fn dispatch_trade(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    match parse_trade_subcommand(sub_matches) {
        TradeSubcommand::Create(sub_sub_matches) => create_trade(trust, sub_sub_matches)?,
        TradeSubcommand::Fund(sub_sub_matches) => create_funding(trust, sub_sub_matches)?,
        TradeSubcommand::Cancel(sub_sub_matches) => create_cancel(trust, sub_sub_matches)?,
        TradeSubcommand::Submit(sub_sub_matches) => create_submit(trust, sub_sub_matches)?,
        TradeSubcommand::ManuallyFill => create_fill(trust),
        TradeSubcommand::ManuallyStop => create_stop(trust),
        TradeSubcommand::ManuallyTarget => create_target(trust),
        TradeSubcommand::ManuallyClose(close_matches) => close(trust, close_matches),
        TradeSubcommand::Sync(sub_sub_matches) => create_sync(trust, sub_sub_matches)?,
        TradeSubcommand::Watch(sub_sub_matches) => {
            trade_queries::create_watch(trust, sub_sub_matches)
        }
        TradeSubcommand::Search(sub_sub_matches) => {
            trade_queries::search_trade(trust, sub_sub_matches)?
        }
        TradeSubcommand::ListOpen(sub_sub_matches) => {
            trade_queries::list_open_trades(trust, sub_sub_matches)?
        }
        TradeSubcommand::Reconcile(sub_sub_matches) => {
            trade_queries::reconcile_trades(trust, sub_sub_matches)?
        }
        TradeSubcommand::ModifyStop => modify_stop(trust),
        TradeSubcommand::ModifyTarget => modify_target(trust),
        TradeSubcommand::SizePreview(sub_sub_matches) => sizing::trade_size_preview(
            trust,
            sub_sub_matches,
            output::parse_report_format(sub_sub_matches),
        )?,
        TradeSubcommand::Hypothesis(sub_sub_matches) => sizing::trade_hypothesis(
            trust,
            sub_sub_matches,
            output::parse_report_format(sub_sub_matches),
        )?,
        TradeSubcommand::Advisor(sub_sub_matches) => trade_advisor::trade_advisor(
            trust,
            sub_sub_matches,
            output::parse_report_format(sub_sub_matches),
        )?,
        TradeSubcommand::Events(sub_sub_matches) => {
            trade_advisor::trade_events(trust, sub_sub_matches)?
        }
        TradeSubcommand::Audit(sub_sub_matches) => {
            trade_queries::trade_audit(trust, sub_sub_matches)?
        }
        TradeSubcommand::Autopsy(sub_sub_matches) => {
            autopsy::trade_autopsy(trust, sub_sub_matches)?
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn create_trade(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    if !has_non_interactive_trade_args(sub_matches) {
        TradeDialogBuilder::new()
            .account(trust)
            .trading_vehicle(trust)
            .category()
            .entry_price()
            .stop_price()
            .safety_order_category()
            .currency(trust)
            .quantity(trust)
            .target_price()
            .thesis()
            .sector()
            .asset_class()
            .context()
            .build(trust)
            .display();
        return Ok(());
    }

    let format = ReportOutputFormat::Text;
    let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
        output::report_error(
            format,
            "missing_argument",
            "Missing --account for non-interactive trade creation",
        )
    })?;
    let symbol_raw = sub_matches.get_one::<String>("symbol").ok_or_else(|| {
        output::report_error(
            format,
            "missing_argument",
            "Missing --symbol for non-interactive trade creation",
        )
    })?;
    let category_raw = sub_matches.get_one::<String>("category").ok_or_else(|| {
        output::report_error(
            format,
            "missing_argument",
            "Missing --category for non-interactive trade creation",
        )
    })?;
    let quantity_raw = sub_matches.get_one::<String>("quantity").ok_or_else(|| {
        output::report_error(
            format,
            "missing_argument",
            "Missing --quantity for non-interactive trade creation",
        )
    })?;

    let account = context::resolve_account_arg(trust, account_raw, format)?;
    let symbol = symbol_raw.trim().to_uppercase();
    let trading_vehicle = trust
            .search_trading_vehicles()
            .map_err(|error| {
                output::report_error(
                    format,
                    "trading_vehicle_lookup_failed",
                    format!("Failed to load trading vehicles: {error}"),
                )
            })?
            .into_iter()
            .find(|vehicle| vehicle.symbol.eq_ignore_ascii_case(&symbol))
            .ok_or_else(|| {
                output::report_error(
                    format,
                    "trading_vehicle_not_found",
                    format!(
                        "Trading vehicle '{symbol}' not found. Create it first (e.g. trust trading-vehicle create --from-alpaca --account {} --symbol {symbol} --confirm-protected <KEYWORD>)",
                        account.name
                    ),
                )
            })?;

    let category = TradeCategory::from_str(category_raw).map_err(|_| {
        output::report_error(
            format,
            "invalid_trade_category",
            format!("Invalid --category value: {category_raw}"),
        )
    })?;

    let entry_price = args::parse_decimal_arg(sub_matches, "entry", format)?;
    let stop_price = args::parse_decimal_arg(sub_matches, "stop", format)?;
    let target_price = args::parse_decimal_arg(sub_matches, "target", format)?;
    let safety_order_category = sub_matches
        .get_one::<String>("safety-order-type")
        .map(|raw| {
            let category = OrderCategory::from_str(raw).map_err(|_| {
                output::report_error(
                    format,
                    "invalid_safety_order_type",
                    format!("Invalid --safety-order-type value: {raw}"),
                )
            })?;
            if matches!(category, OrderCategory::Stop | OrderCategory::StopLimit) {
                Ok(category)
            } else {
                Err(output::report_error(
                    format,
                    "invalid_safety_order_type",
                    format!("Invalid --safety-order-type value: {raw}"),
                ))
            }
        })
        .transpose()?
        .unwrap_or(OrderCategory::Stop);

    let quantity = Decimal::from_str(quantity_raw).map_err(|_| {
        output::report_error(
            format,
            "invalid_quantity",
            format!("Invalid decimal quantity: {quantity_raw}"),
        )
    })?;
    if quantity <= Decimal::ZERO {
        return Err(output::report_error(
            format,
            "invalid_quantity",
            "Quantity must be greater than 0",
        ));
    }

    let currency_raw = sub_matches
        .get_one::<String>("currency")
        .map(std::string::String::as_str)
        .unwrap_or("usd")
        .to_uppercase();
    let currency = Currency::from_str(currency_raw.as_str()).map_err(|_| {
        output::report_error(
            format,
            "invalid_currency",
            format!("Invalid --currency value: {}", currency_raw.to_lowercase()),
        )
    })?;

    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle,
        quantity,
        currency,
        category,
        thesis: sub_matches.get_one::<String>("thesis").cloned(),
        sector: sub_matches.get_one::<String>("sector").cloned(),
        asset_class: sub_matches.get_one::<String>("asset-class").cloned(),
        context: sub_matches.get_one::<String>("context").cloned(),
    };

    let trade = trust
        .create_trade_with_safety_order_category(
            draft,
            stop_price,
            entry_price,
            target_price,
            safety_order_category,
        )
        .map_err(|error| {
            output::report_error(
                format,
                "trade_create_failed",
                format!("Failed to create trade: {error}"),
            )
        })?;

    println!("Trade created:");
    crate::views::TradeView::display(&trade, &account.name);
    crate::views::TradeBalanceView::display(&trade.balance);

    let auto_submit = sub_matches.get_flag("auto-submit");
    let auto_fund = auto_submit || sub_matches.get_flag("auto-fund");

    if auto_fund {
        let (funded_trade, tx, account_balance, trade_balance) =
            trust.fund_trade(&trade).map_err(|error| {
                output::report_error(
                    format,
                    "trade_fund_failed",
                    format!("Failed to fund trade: {error}"),
                )
            })?;
        println!("Trade funded:");
        crate::views::TradeView::display(&funded_trade, &account.name);
        crate::views::TradeBalanceView::display(&trade_balance);
        crate::views::TransactionView::display_with_basis(
            &tx,
            &account.name,
            account_balance.total_balance,
        );
        crate::views::AccountBalanceView::display(account_balance, &account.name);

        if auto_submit {
            let (submitted_trade, log) = trust.submit_trade(&funded_trade).map_err(|error| {
                output::report_error(
                    format,
                    "trade_submit_failed",
                    format!("Failed to submit trade: {error}"),
                )
            })?;
            println!("Trade submitted:");
            crate::views::TradeView::display(&submitted_trade, &account.name);
            crate::views::TradeBalanceView::display(&submitted_trade.balance);
            crate::views::LogView::display(&log);
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn create_cancel(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    if sub_matches.get_one::<String>("trade-id").is_none() {
        CancelDialogBuilder::new()
            .account(trust)
            .search(trust)
            .build(trust)
            .display();
        return Ok(());
    }

    let format = ReportOutputFormat::Text;
    let trade_id = args::parse_uuid_arg(sub_matches, "trade-id", format)?;
    let trade = context::find_trade_by_id(
        trust,
        trade_id,
        &[model::Status::Funded, model::Status::Submitted],
        format,
    )?;
    let account = context::account_by_id(trust, trade.account_id, format)?;

    let (trade_balance, account_balance, transaction) = match trade.status {
        model::Status::Funded => trust.cancel_funded_trade(&trade),
        model::Status::Submitted => trust.cancel_submitted_trade(&trade),
        _ => Err("Trade must be funded or submitted to be canceled".into()),
    }
    .map_err(|error| {
        output::report_error(
            format,
            "trade_cancel_failed",
            format!("Failed to cancel trade: {error}"),
        )
    })?;

    println!("Trade canceled:");
    crate::views::TradeBalanceView::display(&trade_balance);
    crate::views::TransactionView::display_with_basis(
        &transaction,
        &account.name,
        account_balance.total_balance,
    );
    crate::views::AccountBalanceView::display(account_balance, &account.name);
    Ok(())
}

pub(in crate::dispatcher) fn create_funding(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    if sub_matches.get_one::<String>("trade-id").is_none() {
        FundingDialogBuilder::new()
            .account(trust)
            .search(trust)
            .build(trust)
            .display();
        return Ok(());
    }

    let format = ReportOutputFormat::Text;
    let trade_id = args::parse_uuid_arg(sub_matches, "trade-id", format)?;
    let trade = context::find_trade_by_id(trust, trade_id, &[model::Status::New], format)?;
    let account = context::account_by_id(trust, trade.account_id, format)?;

    let (funded_trade, tx, account_balance, trade_balance) =
        trust.fund_trade(&trade).map_err(|error| {
            output::report_error(
                format,
                "trade_fund_failed",
                format!("Failed to fund trade: {error}"),
            )
        })?;

    println!("Trade funded:");
    crate::views::TradeView::display(&funded_trade, &account.name);
    crate::views::TradeBalanceView::display(&trade_balance);
    crate::views::TransactionView::display_with_basis(
        &tx,
        &account.name,
        account_balance.total_balance,
    );
    crate::views::AccountBalanceView::display(account_balance, &account.name);
    Ok(())
}

pub(in crate::dispatcher) fn create_submit(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    if sub_matches.get_one::<String>("trade-id").is_none() {
        SubmitDialogBuilder::new()
            .account(trust)
            .search(trust)
            .build(trust)
            .display();
        return Ok(());
    }

    let format = ReportOutputFormat::Text;
    let trade_id = args::parse_uuid_arg(sub_matches, "trade-id", format)?;
    let trade = context::find_trade_by_id(trust, trade_id, &[model::Status::Funded], format)?;
    let account = context::account_by_id(trust, trade.account_id, format)?;
    let (submitted_trade, log) = trust.submit_trade(&trade).map_err(|error| {
        output::report_error(
            format,
            "trade_submit_failed",
            format!("Failed to submit trade: {error}"),
        )
    })?;

    println!("Trade submitted:");
    crate::views::TradeView::display(&submitted_trade, &account.name);
    crate::views::TradeBalanceView::display(&submitted_trade.balance);
    println!("Stop:");
    crate::views::OrderView::display(submitted_trade.safety_stop);
    println!("Entry:");
    crate::views::OrderView::display(submitted_trade.entry);
    println!("Target:");
    crate::views::OrderView::display(submitted_trade.target);
    crate::views::LogView::display(&log);
    Ok(())
}

pub(in crate::dispatcher) fn create_fill(trust: &mut TrustFacade) {
    FillTradeDialogBuilder::new()
        .account(trust)
        .search(trust)
        .fee()
        .build(trust)
        .display();
}

pub(in crate::dispatcher) fn create_stop(trust: &mut TrustFacade) {
    ExitDialogBuilder::new()
        .account(trust)
        .search(trust)
        .fee()
        .build_stop(trust)
        .display();
}

pub(in crate::dispatcher) fn create_target(trust: &mut TrustFacade) {
    ExitDialogBuilder::new()
        .account(trust)
        .search(trust)
        .fee()
        .build_target(trust)
        .display();
}

pub(in crate::dispatcher) fn create_sync(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    if sub_matches.get_one::<String>("trade-id").is_none() {
        SyncTradeDialogBuilder::new()
            .account(trust)
            .search(trust)
            .build(trust)
            .display();
        return Ok(());
    }

    let format = ReportOutputFormat::Text;
    let trade_id = args::parse_uuid_arg(sub_matches, "trade-id", format)?;
    let trade = context::find_trade_by_id(
        trust,
        trade_id,
        &[
            model::Status::Submitted,
            model::Status::PartiallyFilled,
            model::Status::Filled,
        ],
        format,
    )?;
    let account = context::account_by_id(trust, trade.account_id, format)?;
    let (status, orders, log) = trust.sync_trade(&trade, &account).map_err(|error| {
        output::report_error(
            format,
            "trade_sync_failed",
            format!("Failed to sync trade: {error}"),
        )
    })?;

    println!("Trade synced:");
    println!("Trade id: {}", trade.id);
    println!("New status: {}", status);
    if orders.is_empty() {
        println!("No order updates returned from broker.");
    } else {
        println!("Updated orders:");
        for order in orders {
            crate::views::OrderView::display(order);
        }
    }
    crate::views::LogView::display(&log);
    Ok(())
}

pub(in crate::dispatcher) fn close(trust: &mut TrustFacade, matches: &ArgMatches) {
    // Check if auto-distribute flag is set
    let auto_distribute = matches.get_flag("auto-distribute");

    if auto_distribute {
        println!("🚀 Enhanced trade closure with automatic profit distribution enabled!");
        println!("   If the trade is profitable, profits will be automatically distributed.");
    }

    CloseDialogBuilder::new()
        .account(trust)
        .search(trust)
        .auto_distribute(auto_distribute)
        .build(trust)
        .display();
}

pub(in crate::dispatcher) fn modify_stop(trust: &mut TrustFacade) {
    ModifyDialogBuilder::new()
        .account(trust)
        .search(trust)
        .new_price()
        .build_stop(trust)
        .display();
}

pub(in crate::dispatcher) fn modify_target(trust: &mut TrustFacade) {
    ModifyDialogBuilder::new()
        .account(trust)
        .search(trust)
        .new_price()
        .build_target(trust)
        .display();
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct TradeCommand;
impl super::Command for TradeCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_trade(trust, args)
    }
}
