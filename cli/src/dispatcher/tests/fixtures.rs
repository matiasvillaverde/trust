#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use crate::commands::AccountCommandBuilder;
use crate::commands::AdvisorCommandBuilder;
use crate::commands::DbCommandBuilder;
use crate::commands::DistributionCommandBuilder;
use crate::commands::GradeCommandBuilder;
use crate::commands::KeysCommandBuilder;
use crate::commands::LevelCommandBuilder;
use crate::commands::MarketDataCommandBuilder;
use crate::commands::MetricsCommandBuilder;
use crate::commands::OnboardingCommandBuilder;
use crate::commands::PolicyCommandBuilder;
use crate::commands::ReportCommandBuilder;
use crate::commands::RuleCommandBuilder;
use crate::commands::SessionCommandBuilder;
use crate::commands::TradeCommandBuilder;
use crate::commands::TradingVehicleCommandBuilder;
use crate::commands::TransactionCommandBuilder;
use crate::dispatcher::ArgDispatcher;
use alpaca_broker::AlpacaBroker;
use chrono::Utc;
use clap::Arg;
use clap::Command;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::Broker;
use model::Currency;
use model::Environment;
use model::Mistake;
use model::MistakeErrorType;
use model::MungerTendency;
use model::Status;
use model::Trade;
use model::TradingVehicleCategory;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

pub(super) fn env_guard() -> std::sync::MutexGuard<'static, ()> {
    crate::protected_keyword::test_env_guard()
}

pub(super) fn test_dispatcher() -> ArgDispatcher {
    let trust = TrustFacade::new(
        Box::new(SqliteDatabase::new_in_memory()),
        Box::<AlpacaBroker>::default(),
    );
    ArgDispatcher { trust }
}

pub(super) fn test_dispatcher_with_broker(broker: Box<dyn Broker>) -> ArgDispatcher {
    let trust = TrustFacade::new(Box::new(SqliteDatabase::new_in_memory()), broker);
    ArgDispatcher { trust }
}

pub(super) fn test_dispatcher_with_read_failures(
    factory: crate::test_support::ReadFailureFactory,
) -> ArgDispatcher {
    let trust = TrustFacade::new(Box::new(factory), Box::<AlpacaBroker>::default());
    ArgDispatcher { trust }
}

pub(super) fn account_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("confirm-protected").long("confirm-protected"))
        .arg(Arg::new("name").long("name"))
        .arg(Arg::new("description").long("description"))
        .arg(Arg::new("environment").long("environment"))
        .arg(Arg::new("taxes").long("taxes"))
        .arg(Arg::new("earnings").long("earnings"))
        .arg(Arg::new("type").long("type"))
        .arg(Arg::new("broker").long("broker"))
        .arg(Arg::new("broker-account-id").long("broker-account-id"))
        .arg(Arg::new("parent").long("parent"))
        .arg(Arg::new("account").long("account"))
        .arg(
            Arg::new("force")
                .long("force")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches_from(argv)
}

pub(super) fn trade_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("account").long("account"))
        .arg(Arg::new("symbol").long("symbol"))
        .arg(Arg::new("category").long("category"))
        .arg(Arg::new("entry").long("entry"))
        .arg(Arg::new("stop").long("stop"))
        .arg(Arg::new("safety-order-type").long("safety-order-type"))
        .arg(Arg::new("target").long("target"))
        .arg(Arg::new("quantity").long("quantity"))
        .arg(Arg::new("currency").long("currency"))
        .arg(
            Arg::new("auto-fund")
                .long("auto-fund")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("auto-submit")
                .long("auto-submit")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(Arg::new("thesis").long("thesis"))
        .arg(Arg::new("sector").long("sector"))
        .arg(Arg::new("asset-class").long("asset-class"))
        .arg(Arg::new("context").long("context"))
        .get_matches_from(argv)
}

pub(super) fn trade_event_add_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("trade-id").long("trade-id"))
        .arg(Arg::new("type").long("type"))
        .arg(Arg::new("date").long("date"))
        .arg(Arg::new("severity").long("severity"))
        .arg(Arg::new("notes").long("notes"))
        .get_matches_from(argv)
}

pub(super) fn trade_event_list_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("trade-id").long("trade-id"))
        .arg(Arg::new("format").long("format"))
        .get_matches_from(argv)
}

pub(super) fn trade_autopsy_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("trade-id").long("trade-id"))
        .get_matches_from(argv)
}

pub(super) fn size_preview_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("account").long("account"))
        .arg(Arg::new("entry").long("entry"))
        .arg(Arg::new("stop").long("stop"))
        .arg(Arg::new("currency").long("currency"))
        .get_matches_from(argv)
}

pub(super) fn hypothesis_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("account").long("account"))
        .arg(Arg::new("entry").long("entry"))
        .arg(Arg::new("stop").long("stop"))
        .arg(Arg::new("target").long("target"))
        .arg(Arg::new("quantity").long("quantity"))
        .arg(Arg::new("currency").long("currency"))
        .get_matches_from(argv)
}

pub(super) fn market_stream_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("symbols").long("symbols"))
        .arg(Arg::new("channels").long("channels"))
        .arg(Arg::new("max-events").long("max-events"))
        .arg(Arg::new("timeout-seconds").long("timeout-seconds"))
        .get_matches_from(argv)
}

pub(super) fn market_data_command_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["market-data"];
    argv.extend_from_slice(args);
    MarketDataCommandBuilder::new()
        .snapshot()
        .bars()
        .stream()
        .quote()
        .trade()
        .session()
        .build()
        .get_matches_from(argv)
}

pub(super) fn report_args_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("account").long("account"))
        .arg(
            Arg::new("days")
                .long("days")
                .value_parser(clap::value_parser!(u32)),
        )
        .arg(
            Arg::new("open-only")
                .long("open-only")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches_from(argv)
}

pub(super) fn advisor_configure_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("confirm-protected").long("confirm-protected"))
        .arg(Arg::new("account").long("account"))
        .arg(Arg::new("sector-limit").long("sector-limit"))
        .arg(Arg::new("asset-class-limit").long("asset-class-limit"))
        .arg(Arg::new("single-position-limit").long("single-position-limit"))
        .arg(Arg::new("calendar-provider").long("calendar-provider"))
        .arg(Arg::new("calendar-api-key").long("calendar-api-key"))
        .arg(Arg::new("claude-api-key").long("claude-api-key"))
        .arg(
            Arg::new("show")
                .long("show")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches_from(argv)
}

pub(super) fn advisor_check_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("account").long("account"))
        .arg(Arg::new("symbol").long("symbol"))
        .arg(Arg::new("entry").long("entry"))
        .arg(Arg::new("quantity").long("quantity"))
        .arg(Arg::new("sector").long("sector"))
        .arg(Arg::new("asset-class").long("asset-class"))
        .get_matches_from(argv)
}

pub(super) fn advisor_history_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("account").long("account"))
        .arg(Arg::new("days").long("days"))
        .get_matches_from(argv)
}

pub(super) fn session_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("account").long("account"))
        .arg(Arg::new("format").long("format"))
        .get_matches_from(argv)
}

pub(super) fn distribution_config_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("account-id").long("account-id"))
        .arg(Arg::new("earnings").long("earnings"))
        .arg(Arg::new("tax").long("tax"))
        .arg(Arg::new("reinvestment").long("reinvestment"))
        .arg(Arg::new("insurance").long("insurance"))
        .arg(Arg::new("threshold").long("threshold"))
        .arg(Arg::new("password").long("password"))
        .get_matches_from(argv)
}

pub(super) fn distribution_execute_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("account-id").long("account-id"))
        .arg(Arg::new("amount").long("amount"))
        .get_matches_from(argv)
}

pub(super) fn distribution_history_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("account-id").long("account-id"))
        .arg(Arg::new("limit").long("limit"))
        .get_matches_from(argv)
}

pub(super) fn distribution_rules_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("account-id").long("account-id"))
        .get_matches_from(argv)
}

pub(super) fn transfer_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(args);
    Command::new("test")
        .arg(Arg::new("from").long("from"))
        .arg(Arg::new("to").long("to"))
        .arg(Arg::new("amount").long("amount"))
        .arg(Arg::new("reason").long("reason"))
        .get_matches_from(argv)
}

pub(super) fn root_command() -> Command {
    Command::new("trust")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .allow_external_subcommands(true)
        .subcommand(DbCommandBuilder::new().export().import().build())
        .subcommand(
            KeysCommandBuilder::new()
                .create_keys()
                .read_environment()
                .delete_environment()
                .protected_set()
                .protected_show()
                .protected_delete()
                .build(),
        )
        .subcommand(
            AccountCommandBuilder::new()
                .create_account()
                .read_account()
                .list_accounts()
                .balance_accounts()
                .transfer_account()
                .delete_account()
                .build(),
        )
        .subcommand(
            TransactionCommandBuilder::new()
                .deposit()
                .withdraw()
                .build(),
        )
        .subcommand(
            RuleCommandBuilder::new()
                .create_rule()
                .remove_rule()
                .build(),
        )
        .subcommand(
            TradingVehicleCommandBuilder::new()
                .create_trading_vehicle()
                .update_bond_terms()
                .search_trading_vehicle()
                .build(),
        )
        .subcommand(
            TradeCommandBuilder::new()
                .create_trade()
                .search_trade()
                .list_open()
                .reconcile()
                .fund_trade()
                .cancel_trade()
                .submit_trade()
                .sync_trade()
                .watch_trade()
                .manually_fill()
                .manually_stop()
                .manually_target()
                .manually_close()
                .modify_stop()
                .modify_target()
                .size_preview()
                .build(),
        )
        .subcommand(
            DistributionCommandBuilder::new()
                .configure_distribution()
                .execute_distribution()
                .history()
                .insurance_status()
                .show_rules()
                .build(),
        )
        .subcommand(
            ReportCommandBuilder::new()
                .performance()
                .drawdown()
                .risk()
                .concentration()
                .summary()
                .metrics()
                .attribution()
                .benchmark()
                .timeline()
                .bias_summary()
                .build(),
        )
        .subcommand(
            MarketDataCommandBuilder::new()
                .snapshot()
                .bars()
                .stream()
                .quote()
                .trade()
                .session()
                .build(),
        )
        .subcommand(GradeCommandBuilder::new().show().summary().build())
        .subcommand(
            LevelCommandBuilder::new()
                .status()
                .triggers()
                .history()
                .change()
                .evaluate()
                .progress()
                .rules()
                .build(),
        )
        .subcommand(
            MetricsCommandBuilder::new()
                .advanced()
                .compare()
                .bond()
                .build(),
        )
        .subcommand(
            AdvisorCommandBuilder::new()
                .configure()
                .check()
                .status()
                .history()
                .build(),
        )
        .subcommand(SessionCommandBuilder::new().open().close().list().build())
        .subcommand(OnboardingCommandBuilder::new().init().status().build())
        .subcommand(PolicyCommandBuilder::new().build())
}

pub(super) fn seed_account_and_vehicle(
    dispatcher: &mut ArgDispatcher,
) -> (model::Account, model::TradingVehicle) {
    let account = dispatcher
        .trust
        .create_account("disp-acct", "test", Environment::Paper, dec!(20), dec!(10))
        .expect("account");
    dispatcher
        .trust
        .create_transaction(
            &account,
            &model::TransactionCategory::Deposit,
            dec!(10_000),
            &Currency::USD,
        )
        .expect("deposit");
    let vehicle = dispatcher
        .trust
        .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
        .expect("vehicle");
    (account, vehicle)
}

pub(super) fn seed_new_trade(dispatcher: &mut ArgDispatcher) -> (model::Account, Trade) {
    let (account, vehicle) = seed_account_and_vehicle(dispatcher);
    let trade = dispatcher
        .trust
        .create_trade(
            model::DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle,
                quantity: 2.into(),
                category: model::TradeCategory::Long,
                currency: Currency::USD,
                thesis: Some("breakout".to_string()),
                sector: Some("technology".to_string()),
                asset_class: Some("equity".to_string()),
                context: None,
            },
            dec!(95),
            dec!(100),
            dec!(110),
        )
        .expect("seed trade");
    (account, trade)
}

pub(super) fn seed_filled_trade(dispatcher: &mut ArgDispatcher) -> (model::Account, Trade) {
    let (account, trade) = seed_new_trade(dispatcher);
    let (funded, _, _, _) = dispatcher.trust.fund_trade(&trade).expect("fund trade");
    let (submitted, _) = dispatcher
        .trust
        .submit_trade(&funded)
        .expect("submit trade");
    let (status, _, _) = dispatcher
        .trust
        .sync_trade(&submitted, &account)
        .expect("sync trade to filled");
    assert_eq!(status, Status::Filled);
    let filled = dispatcher
        .trust
        .search_trades(account.id, Status::Filled)
        .expect("load filled trades")
        .into_iter()
        .find(|candidate| candidate.id == trade.id)
        .expect("filled trade should be persisted");
    (account, filled)
}

pub(super) fn seed_closed_target_trade(dispatcher: &mut ArgDispatcher) -> (model::Account, Trade) {
    let (account, filled) = seed_filled_trade(dispatcher);
    let mut target_ready = filled.clone();
    target_ready.target.status = model::OrderStatus::Filled;
    target_ready.target.filled_quantity = target_ready.target.quantity;
    target_ready.target.average_filled_price = Some(dec!(110));
    target_ready.target.filled_at = Some(Utc::now().naive_utc());
    dispatcher
        .trust
        .target_acquired(&target_ready, Decimal::ZERO)
        .expect("seed closed target trade");
    let closed = dispatcher
        .trust
        .search_trades(account.id, Status::ClosedTarget)
        .expect("load closed target trades")
        .into_iter()
        .find(|candidate| candidate.id == filled.id)
        .expect("closed trade should be persisted");
    (account, closed)
}

pub(super) fn seed_mistake(
    dispatcher: &mut ArgDispatcher,
    trade_id: Uuid,
    bias_tags: Vec<MungerTendency>,
    lollapalooza: bool,
    error_type: MistakeErrorType,
) -> Mistake {
    let now = Utc::now().naive_utc();
    dispatcher
        .trust
        .create_mistake(Mistake {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id,
            bias_tags,
            lollapalooza,
            error_type,
            rule_violated: None,
            counterfactual_r: Decimal::ZERO,
            lesson: "Review the process rule before taking the next setup.".to_string(),
        })
        .expect("mistake should persist")
}

pub(super) fn level_eval_matches(args: &[&str]) -> clap::ArgMatches {
    let mut argv: Vec<&str> = vec!["level-eval"];
    argv.extend_from_slice(args);
    Command::new("level-eval")
        .arg(
            Arg::new("profitable-trades")
                .long("profitable-trades")
                .value_parser(clap::value_parser!(u32)),
        )
        .arg(Arg::new("win-rate").long("win-rate"))
        .arg(Arg::new("monthly-loss").long("monthly-loss"))
        .arg(Arg::new("largest-loss").long("largest-loss"))
        .arg(
            Arg::new("consecutive-wins")
                .long("consecutive-wins")
                .value_parser(clap::value_parser!(u32)),
        )
        .get_matches_from(argv)
}
