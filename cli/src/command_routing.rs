use clap::ArgMatches;

pub enum TopLevelCommand<'a> {
    Db(&'a ArgMatches),
    Keys(&'a ArgMatches),
    Accounts(&'a ArgMatches),
    Transaction(&'a ArgMatches),
    Rule(&'a ArgMatches),
    TradingVehicle(&'a ArgMatches),
    Trade(&'a ArgMatches),
    Distribution(&'a ArgMatches),
    Report(&'a ArgMatches),
    MarketData(&'a ArgMatches),
    Grade(&'a ArgMatches),
    Level(&'a ArgMatches),
    Onboarding(&'a ArgMatches),
    Policy(&'a ArgMatches),
    Metrics(&'a ArgMatches),
    Advisor(&'a ArgMatches),
    External { name: &'a str, args: &'a ArgMatches },
}

pub enum DbSubcommand<'a> {
    Export(&'a ArgMatches),
    Import(&'a ArgMatches),
}

pub enum KeysSubcommand<'a> {
    Create(&'a ArgMatches),
    Show,
    Delete(&'a ArgMatches),
    ProtectedSet(&'a ArgMatches),
    ProtectedShow,
    ProtectedDelete(&'a ArgMatches),
}

pub enum AccountsSubcommand<'a> {
    Create(&'a ArgMatches),
    Search,
    List(&'a ArgMatches),
    Balance(&'a ArgMatches),
    Transfer(&'a ArgMatches),
}

pub enum TransactionsSubcommand<'a> {
    Deposit(&'a ArgMatches),
    Withdraw(&'a ArgMatches),
}

pub enum RulesSubcommand<'a> {
    Create(&'a ArgMatches),
    Remove(&'a ArgMatches),
}

pub enum TradingVehicleSubcommand<'a> {
    Create(&'a ArgMatches),
    Search,
}

pub enum TradeSubcommand<'a> {
    Create(&'a ArgMatches),
    Fund(&'a ArgMatches),
    Cancel(&'a ArgMatches),
    Submit(&'a ArgMatches),
    ManuallyFill,
    ManuallyStop,
    ManuallyTarget,
    ManuallyClose(&'a ArgMatches),
    Sync(&'a ArgMatches),
    Watch(&'a ArgMatches),
    Search(&'a ArgMatches),
    ListOpen(&'a ArgMatches),
    Reconcile(&'a ArgMatches),
    ModifyStop,
    ModifyTarget,
    SizePreview(&'a ArgMatches),
}

pub enum DistributionSubcommand<'a> {
    Configure(&'a ArgMatches),
    Execute(&'a ArgMatches),
    History(&'a ArgMatches),
    Rules(&'a ArgMatches),
}

pub enum ReportSubcommand<'a> {
    Drawdown(&'a ArgMatches),
    Performance(&'a ArgMatches),
    Risk(&'a ArgMatches),
    Concentration(&'a ArgMatches),
    Summary(&'a ArgMatches),
    Metrics(&'a ArgMatches),
    Attribution(&'a ArgMatches),
    Benchmark(&'a ArgMatches),
    Timeline(&'a ArgMatches),
}

pub enum MarketDataSubcommand<'a> {
    Snapshot(&'a ArgMatches),
    Bars(&'a ArgMatches),
    Stream(&'a ArgMatches),
    Quote(&'a ArgMatches),
    Trade(&'a ArgMatches),
    Session(&'a ArgMatches),
}

pub enum GradeSubcommand<'a> {
    Show(&'a ArgMatches),
    Summary(&'a ArgMatches),
}

pub enum LevelSubcommand<'a> {
    Triggers(&'a ArgMatches),
    Status(&'a ArgMatches),
    History(&'a ArgMatches),
    Change(&'a ArgMatches),
    Evaluate(&'a ArgMatches),
    Progress(&'a ArgMatches),
    RulesShow(&'a ArgMatches),
    RulesSet(&'a ArgMatches),
}

pub enum OnboardingSubcommand<'a> {
    Init(&'a ArgMatches),
    Status,
}

pub enum MetricsSubcommand<'a> {
    Advanced(&'a ArgMatches),
    Compare(&'a ArgMatches),
}

pub enum AdvisorSubcommand<'a> {
    Configure(&'a ArgMatches),
    Check(&'a ArgMatches),
    Status(&'a ArgMatches),
    History(&'a ArgMatches),
}

pub fn parse_top_level_command(matches: &ArgMatches) -> TopLevelCommand<'_> {
    match matches.subcommand() {
        Some(("db", sub_matches)) => TopLevelCommand::Db(sub_matches),
        Some(("keys", sub_matches)) => TopLevelCommand::Keys(sub_matches),
        Some(("account", sub_matches)) | Some(("accounts", sub_matches)) => {
            TopLevelCommand::Accounts(sub_matches)
        }
        Some(("transaction", sub_matches)) => TopLevelCommand::Transaction(sub_matches),
        Some(("rule", sub_matches)) => TopLevelCommand::Rule(sub_matches),
        Some(("trading-vehicle", sub_matches)) => TopLevelCommand::TradingVehicle(sub_matches),
        Some(("trade", sub_matches)) => TopLevelCommand::Trade(sub_matches),
        Some(("distribution", sub_matches)) => TopLevelCommand::Distribution(sub_matches),
        Some(("report", sub_matches)) => TopLevelCommand::Report(sub_matches),
        Some(("market-data", sub_matches)) => TopLevelCommand::MarketData(sub_matches),
        Some(("grade", sub_matches)) => TopLevelCommand::Grade(sub_matches),
        Some(("level", sub_matches)) => TopLevelCommand::Level(sub_matches),
        Some(("onboarding", sub_matches)) => TopLevelCommand::Onboarding(sub_matches),
        Some(("policy", sub_matches)) => TopLevelCommand::Policy(sub_matches),
        Some(("metrics", sub_matches)) => TopLevelCommand::Metrics(sub_matches),
        Some(("advisor", sub_matches)) => TopLevelCommand::Advisor(sub_matches),
        Some((name, sub_matches)) => TopLevelCommand::External {
            name,
            args: sub_matches,
        },
        _ => unreachable!(),
    }
}

pub fn parse_db_subcommand(sub_matches: &ArgMatches) -> DbSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("export", sub_sub_matches)) => DbSubcommand::Export(sub_sub_matches),
        Some(("import", sub_sub_matches)) => DbSubcommand::Import(sub_sub_matches),
        _ => unreachable!("No subcommand provided"),
    }
}

pub fn parse_keys_subcommand(sub_matches: &ArgMatches) -> KeysSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("create", sub_sub_matches)) => KeysSubcommand::Create(sub_sub_matches),
        Some(("show", _)) => KeysSubcommand::Show,
        Some(("delete", sub_sub_matches)) => KeysSubcommand::Delete(sub_sub_matches),
        Some(("protected-set", sub_sub_matches)) => KeysSubcommand::ProtectedSet(sub_sub_matches),
        Some(("protected-show", _)) => KeysSubcommand::ProtectedShow,
        Some(("protected-delete", sub_sub_matches)) => {
            KeysSubcommand::ProtectedDelete(sub_sub_matches)
        }
        _ => unreachable!("No subcommand provided"),
    }
}

pub fn parse_accounts_subcommand(sub_matches: &ArgMatches) -> AccountsSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("create", sub_sub_matches)) => AccountsSubcommand::Create(sub_sub_matches),
        Some(("search", _)) => AccountsSubcommand::Search,
        Some(("list", sub_sub_matches)) => AccountsSubcommand::List(sub_sub_matches),
        Some(("balance", sub_sub_matches)) => AccountsSubcommand::Balance(sub_sub_matches),
        Some(("transfer", sub_sub_matches)) => AccountsSubcommand::Transfer(sub_sub_matches),
        _ => unreachable!("No subcommand provided"),
    }
}

pub fn parse_transactions_subcommand(sub_matches: &ArgMatches) -> TransactionsSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("deposit", sub_sub_matches)) => TransactionsSubcommand::Deposit(sub_sub_matches),
        Some(("withdraw", sub_sub_matches)) => TransactionsSubcommand::Withdraw(sub_sub_matches),
        _ => unreachable!("No subcommand provided"),
    }
}

pub fn parse_rules_subcommand(sub_matches: &ArgMatches) -> RulesSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("create", sub_sub_matches)) => RulesSubcommand::Create(sub_sub_matches),
        Some(("remove", sub_sub_matches)) => RulesSubcommand::Remove(sub_sub_matches),
        _ => unreachable!("No subcommand provided"),
    }
}

pub fn parse_trading_vehicle_subcommand(sub_matches: &ArgMatches) -> TradingVehicleSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("create", sub_sub_matches)) => TradingVehicleSubcommand::Create(sub_sub_matches),
        Some(("search", _)) => TradingVehicleSubcommand::Search,
        _ => unreachable!("No subcommand provided"),
    }
}

pub fn parse_trade_subcommand(sub_matches: &ArgMatches) -> TradeSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("create", sub_sub_matches)) => TradeSubcommand::Create(sub_sub_matches),
        Some(("fund", sub_sub_matches)) => TradeSubcommand::Fund(sub_sub_matches),
        Some(("cancel", sub_sub_matches)) => TradeSubcommand::Cancel(sub_sub_matches),
        Some(("submit", sub_sub_matches)) => TradeSubcommand::Submit(sub_sub_matches),
        Some(("manually-fill", _)) => TradeSubcommand::ManuallyFill,
        Some(("manually-stop", _)) => TradeSubcommand::ManuallyStop,
        Some(("manually-target", _)) => TradeSubcommand::ManuallyTarget,
        Some(("manually-close", sub_sub_matches)) => {
            TradeSubcommand::ManuallyClose(sub_sub_matches)
        }
        Some(("sync", sub_sub_matches)) => TradeSubcommand::Sync(sub_sub_matches),
        Some(("watch", sub_sub_matches)) => TradeSubcommand::Watch(sub_sub_matches),
        Some(("search", sub_sub_matches)) => TradeSubcommand::Search(sub_sub_matches),
        Some(("list-open", sub_sub_matches)) => TradeSubcommand::ListOpen(sub_sub_matches),
        Some(("reconcile", sub_sub_matches)) => TradeSubcommand::Reconcile(sub_sub_matches),
        Some(("modify-stop", _)) => TradeSubcommand::ModifyStop,
        Some(("modify-target", _)) => TradeSubcommand::ModifyTarget,
        Some(("size-preview", sub_sub_matches)) => TradeSubcommand::SizePreview(sub_sub_matches),
        _ => unreachable!("No subcommand provided"),
    }
}

pub fn parse_distribution_subcommand(sub_matches: &ArgMatches) -> DistributionSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("configure", sub_sub_matches)) => DistributionSubcommand::Configure(sub_sub_matches),
        Some(("execute", sub_sub_matches)) => DistributionSubcommand::Execute(sub_sub_matches),
        Some(("history", sub_sub_matches)) => DistributionSubcommand::History(sub_sub_matches),
        Some(("rules", sub_sub_matches)) => DistributionSubcommand::Rules(sub_sub_matches),
        _ => unreachable!("No subcommand provided"),
    }
}

pub fn parse_report_subcommand(sub_matches: &ArgMatches) -> ReportSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("drawdown", sub_sub_matches)) => ReportSubcommand::Drawdown(sub_sub_matches),
        Some(("performance", sub_sub_matches)) => ReportSubcommand::Performance(sub_sub_matches),
        Some(("risk", sub_sub_matches)) => ReportSubcommand::Risk(sub_sub_matches),
        Some(("concentration", sub_sub_matches)) => {
            ReportSubcommand::Concentration(sub_sub_matches)
        }
        Some(("summary", sub_sub_matches)) => ReportSubcommand::Summary(sub_sub_matches),
        Some(("metrics", sub_sub_matches)) => ReportSubcommand::Metrics(sub_sub_matches),
        Some(("attribution", sub_sub_matches)) => ReportSubcommand::Attribution(sub_sub_matches),
        Some(("benchmark", sub_sub_matches)) => ReportSubcommand::Benchmark(sub_sub_matches),
        Some(("timeline", sub_sub_matches)) => ReportSubcommand::Timeline(sub_sub_matches),
        _ => unreachable!("No subcommand provided"),
    }
}

pub fn parse_market_data_subcommand(sub_matches: &ArgMatches) -> MarketDataSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("snapshot", sub_sub_matches)) => MarketDataSubcommand::Snapshot(sub_sub_matches),
        Some(("bars", sub_sub_matches)) => MarketDataSubcommand::Bars(sub_sub_matches),
        Some(("stream", sub_sub_matches)) => MarketDataSubcommand::Stream(sub_sub_matches),
        Some(("quote", sub_sub_matches)) => MarketDataSubcommand::Quote(sub_sub_matches),
        Some(("trade", sub_sub_matches)) => MarketDataSubcommand::Trade(sub_sub_matches),
        Some(("session", sub_sub_matches)) => MarketDataSubcommand::Session(sub_sub_matches),
        _ => unreachable!("No subcommand provided"),
    }
}

pub fn parse_grade_subcommand(sub_matches: &ArgMatches) -> GradeSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("show", sub_sub_matches)) => GradeSubcommand::Show(sub_sub_matches),
        Some(("summary", sub_sub_matches)) => GradeSubcommand::Summary(sub_sub_matches),
        _ => unreachable!("No subcommand provided"),
    }
}

pub fn parse_level_subcommand(sub_matches: &ArgMatches) -> LevelSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("triggers", sub_sub_matches)) => LevelSubcommand::Triggers(sub_sub_matches),
        Some(("status", sub_sub_matches)) => LevelSubcommand::Status(sub_sub_matches),
        Some(("history", sub_sub_matches)) => LevelSubcommand::History(sub_sub_matches),
        Some(("change", sub_sub_matches)) => LevelSubcommand::Change(sub_sub_matches),
        Some(("evaluate", sub_sub_matches)) => LevelSubcommand::Evaluate(sub_sub_matches),
        Some(("progress", sub_sub_matches)) => LevelSubcommand::Progress(sub_sub_matches),
        Some(("rules", sub_sub_matches)) => match sub_sub_matches.subcommand() {
            Some(("show", nested)) => LevelSubcommand::RulesShow(nested),
            Some(("set", nested)) => LevelSubcommand::RulesSet(nested),
            _ => unreachable!("No subcommand provided"),
        },
        _ => unreachable!("No subcommand provided"),
    }
}

pub fn parse_onboarding_subcommand(sub_matches: &ArgMatches) -> OnboardingSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("init", sub_sub_matches)) => OnboardingSubcommand::Init(sub_sub_matches),
        Some(("status", _)) => OnboardingSubcommand::Status,
        _ => unreachable!("No subcommand provided"),
    }
}

pub fn parse_metrics_subcommand(sub_matches: &ArgMatches) -> MetricsSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("advanced", sub_sub_matches)) => MetricsSubcommand::Advanced(sub_sub_matches),
        Some(("compare", sub_sub_matches)) => MetricsSubcommand::Compare(sub_sub_matches),
        _ => unreachable!("No subcommand provided"),
    }
}

pub fn parse_advisor_subcommand(sub_matches: &ArgMatches) -> AdvisorSubcommand<'_> {
    match sub_matches.subcommand() {
        Some(("configure", sub_sub_matches)) => AdvisorSubcommand::Configure(sub_sub_matches),
        Some(("check", sub_sub_matches)) => AdvisorSubcommand::Check(sub_sub_matches),
        Some(("status", sub_sub_matches)) => AdvisorSubcommand::Status(sub_sub_matches),
        Some(("history", sub_sub_matches)) => AdvisorSubcommand::History(sub_sub_matches),
        _ => unreachable!("No subcommand provided"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Arg, Command};

    #[test]
    fn parse_top_level_command_maps_known_and_external() {
        let app = Command::new("trust")
            .subcommand(Command::new("db"))
            .subcommand(Command::new("account"))
            .allow_external_subcommands(true);
        let db = app.clone().get_matches_from(["trust", "db"]);
        assert!(matches!(
            parse_top_level_command(&db),
            TopLevelCommand::Db(_)
        ));
        let account = app.clone().get_matches_from(["trust", "account"]);
        assert!(matches!(
            parse_top_level_command(&account),
            TopLevelCommand::Accounts(_)
        ));
        let ext = app.get_matches_from(["trust", "foo", "bar"]);
        assert!(matches!(
            parse_top_level_command(&ext),
            TopLevelCommand::External { name: "foo", .. }
        ));
    }

    #[test]
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn parse_nested_dispatchers_cover_all_variants() {
        let app = Command::new("trust")
            .subcommand(Command::new("db").subcommand(Command::new("export")))
            .subcommand(Command::new("keys").subcommand(Command::new("protected-show")))
            .subcommand(Command::new("account").subcommand(Command::new("balance")))
            .subcommand(Command::new("transaction").subcommand(Command::new("deposit")))
            .subcommand(Command::new("rule").subcommand(Command::new("remove")))
            .subcommand(Command::new("trading-vehicle").subcommand(Command::new("search")))
            .subcommand(Command::new("trade").subcommand(Command::new("size-preview")))
            .subcommand(Command::new("distribution").subcommand(Command::new("history")))
            .subcommand(Command::new("report").subcommand(Command::new("summary")))
            .subcommand(Command::new("market-data").subcommand(Command::new("bars")))
            .subcommand(Command::new("grade").subcommand(Command::new("show")))
            .subcommand(
                Command::new("level")
                    .subcommand(Command::new("rules").subcommand(Command::new("set"))),
            )
            .subcommand(Command::new("onboarding").subcommand(Command::new("init")))
            .subcommand(Command::new("metrics").subcommand(Command::new("compare")))
            .subcommand(Command::new("advisor").subcommand(Command::new("check")))
            .subcommand(Command::new("policy").arg(Arg::new("noop")));

        let db = app.clone().get_matches_from(["trust", "db", "export"]);
        let keys = app
            .clone()
            .get_matches_from(["trust", "keys", "protected-show"]);
        let account = app
            .clone()
            .get_matches_from(["trust", "account", "balance"]);
        let tx = app
            .clone()
            .get_matches_from(["trust", "transaction", "deposit"]);
        let rule = app.clone().get_matches_from(["trust", "rule", "remove"]);
        let vehicle = app
            .clone()
            .get_matches_from(["trust", "trading-vehicle", "search"]);
        let trade = app
            .clone()
            .get_matches_from(["trust", "trade", "size-preview"]);
        let dist = app
            .clone()
            .get_matches_from(["trust", "distribution", "history"]);
        let report = app.clone().get_matches_from(["trust", "report", "summary"]);
        let market = app
            .clone()
            .get_matches_from(["trust", "market-data", "bars"]);
        let grade = app.clone().get_matches_from(["trust", "grade", "show"]);
        let level = app
            .clone()
            .get_matches_from(["trust", "level", "rules", "set"]);
        let onboarding = app
            .clone()
            .get_matches_from(["trust", "onboarding", "init"]);
        let metrics = app
            .clone()
            .get_matches_from(["trust", "metrics", "compare"]);
        let advisor = app.clone().get_matches_from(["trust", "advisor", "check"]);

        let (_, db_m) = db.subcommand().expect("expected db subcommand");
        assert!(matches!(parse_db_subcommand(db_m), DbSubcommand::Export(_)));
        let (_, keys_m) = keys.subcommand().expect("expected keys subcommand");
        assert!(matches!(
            parse_keys_subcommand(keys_m),
            KeysSubcommand::ProtectedShow
        ));
        let (_, account_m) = account.subcommand().expect("expected account subcommand");
        assert!(matches!(
            parse_accounts_subcommand(account_m),
            AccountsSubcommand::Balance(_)
        ));
        let (_, tx_m) = tx.subcommand().expect("expected transaction subcommand");
        assert!(matches!(
            parse_transactions_subcommand(tx_m),
            TransactionsSubcommand::Deposit(_)
        ));
        let (_, rule_m) = rule.subcommand().expect("expected rule subcommand");
        assert!(matches!(
            parse_rules_subcommand(rule_m),
            RulesSubcommand::Remove(_)
        ));
        let (_, vehicle_m) = vehicle
            .subcommand()
            .expect("expected trading-vehicle subcommand");
        assert!(matches!(
            parse_trading_vehicle_subcommand(vehicle_m),
            TradingVehicleSubcommand::Search
        ));
        let (_, trade_m) = trade.subcommand().expect("expected trade subcommand");
        assert!(matches!(
            parse_trade_subcommand(trade_m),
            TradeSubcommand::SizePreview(_)
        ));
        let (_, dist_m) = dist.subcommand().expect("expected distribution subcommand");
        assert!(matches!(
            parse_distribution_subcommand(dist_m),
            DistributionSubcommand::History(_)
        ));
        let (_, report_m) = report.subcommand().expect("expected report subcommand");
        assert!(matches!(
            parse_report_subcommand(report_m),
            ReportSubcommand::Summary(_)
        ));
        let (_, market_m) = market
            .subcommand()
            .expect("expected market-data subcommand");
        assert!(matches!(
            parse_market_data_subcommand(market_m),
            MarketDataSubcommand::Bars(_)
        ));
        let (_, grade_m) = grade.subcommand().expect("expected grade subcommand");
        assert!(matches!(
            parse_grade_subcommand(grade_m),
            GradeSubcommand::Show(_)
        ));
        let (_, level_m) = level.subcommand().expect("expected level subcommand");
        assert!(matches!(
            parse_level_subcommand(level_m),
            LevelSubcommand::RulesSet(_)
        ));
        let (_, onboarding_m) = onboarding
            .subcommand()
            .expect("expected onboarding subcommand");
        assert!(matches!(
            parse_onboarding_subcommand(onboarding_m),
            OnboardingSubcommand::Init(_)
        ));
        let (_, metrics_m) = metrics.subcommand().expect("expected metrics subcommand");
        assert!(matches!(
            parse_metrics_subcommand(metrics_m),
            MetricsSubcommand::Compare(_)
        ));
        let (_, advisor_m) = advisor.subcommand().expect("expected advisor subcommand");
        assert!(matches!(
            parse_advisor_subcommand(advisor_m),
            AdvisorSubcommand::Check(_)
        ));
    }

    #[test]
    fn parse_db_and_trade_cover_second_variants() {
        let app = Command::new("trust").subcommand(
            Command::new("db")
                .subcommand(Command::new("export"))
                .subcommand(Command::new("import")),
        );
        let db = app.get_matches_from(["trust", "db", "import"]);
        let (_, db_m) = db.subcommand().expect("expected db");
        assert!(matches!(parse_db_subcommand(db_m), DbSubcommand::Import(_)));

        let trade_app = Command::new("trust").subcommand(
            Command::new("trade")
                .subcommand(Command::new("create"))
                .subcommand(Command::new("fund"))
                .subcommand(Command::new("cancel"))
                .subcommand(Command::new("submit"))
                .subcommand(Command::new("manually-fill"))
                .subcommand(Command::new("manually-stop"))
                .subcommand(Command::new("manually-target"))
                .subcommand(Command::new("manually-close"))
                .subcommand(Command::new("sync"))
                .subcommand(Command::new("watch"))
                .subcommand(Command::new("search"))
                .subcommand(Command::new("list-open"))
                .subcommand(Command::new("reconcile"))
                .subcommand(Command::new("modify-stop"))
                .subcommand(Command::new("modify-target"))
                .subcommand(Command::new("size-preview")),
        );
        let trade_variants = [
            ("create", "Create"),
            ("fund", "Fund"),
            ("cancel", "Cancel"),
            ("submit", "Submit"),
            ("manually-fill", "ManuallyFill"),
            ("manually-stop", "ManuallyStop"),
            ("manually-target", "ManuallyTarget"),
            ("manually-close", "ManuallyClose"),
            ("sync", "Sync"),
            ("watch", "Watch"),
            ("search", "Search"),
            ("list-open", "ListOpen"),
            ("reconcile", "Reconcile"),
            ("modify-stop", "ModifyStop"),
            ("modify-target", "ModifyTarget"),
            ("size-preview", "SizePreview"),
        ];
        for (sub, expected) in trade_variants {
            let m = trade_app.clone().get_matches_from(["trust", "trade", sub]);
            let (_, tm) = m.subcommand().expect("expected trade");
            let parsed = parse_trade_subcommand(tm);
            let got = match parsed {
                TradeSubcommand::Create(_) => "Create",
                TradeSubcommand::Fund(_) => "Fund",
                TradeSubcommand::Cancel(_) => "Cancel",
                TradeSubcommand::Submit(_) => "Submit",
                TradeSubcommand::ManuallyFill => "ManuallyFill",
                TradeSubcommand::ManuallyStop => "ManuallyStop",
                TradeSubcommand::ManuallyTarget => "ManuallyTarget",
                TradeSubcommand::ManuallyClose(_) => "ManuallyClose",
                TradeSubcommand::Sync(_) => "Sync",
                TradeSubcommand::Watch(_) => "Watch",
                TradeSubcommand::Search(_) => "Search",
                TradeSubcommand::ListOpen(_) => "ListOpen",
                TradeSubcommand::Reconcile(_) => "Reconcile",
                TradeSubcommand::ModifyStop => "ModifyStop",
                TradeSubcommand::ModifyTarget => "ModifyTarget",
                TradeSubcommand::SizePreview(_) => "SizePreview",
            };
            assert_eq!(got, expected);
        }
    }

    #[test]
    fn parse_top_level_command_covers_all_named_variants() {
        let app = Command::new("trust")
            .allow_external_subcommands(true)
            .subcommand(Command::new("db"))
            .subcommand(Command::new("keys"))
            .subcommand(Command::new("account"))
            .subcommand(Command::new("accounts"))
            .subcommand(Command::new("transaction"))
            .subcommand(Command::new("rule"))
            .subcommand(Command::new("trading-vehicle"))
            .subcommand(Command::new("trade"))
            .subcommand(Command::new("distribution"))
            .subcommand(Command::new("report"))
            .subcommand(Command::new("market-data"))
            .subcommand(Command::new("grade"))
            .subcommand(Command::new("level"))
            .subcommand(Command::new("onboarding"))
            .subcommand(Command::new("policy"))
            .subcommand(Command::new("metrics"))
            .subcommand(Command::new("advisor"));

        let cases = [
            ("db", "db"),
            ("keys", "keys"),
            ("account", "accounts"),
            ("accounts", "accounts"),
            ("transaction", "transaction"),
            ("rule", "rule"),
            ("trading-vehicle", "trading-vehicle"),
            ("trade", "trade"),
            ("distribution", "distribution"),
            ("report", "report"),
            ("market-data", "market-data"),
            ("grade", "grade"),
            ("level", "level"),
            ("onboarding", "onboarding"),
            ("policy", "policy"),
            ("metrics", "metrics"),
            ("advisor", "advisor"),
            ("external-command", "external"),
        ];

        for (name, expected) in cases {
            let matches = app.clone().get_matches_from(["trust", name]);
            let got = match parse_top_level_command(&matches) {
                TopLevelCommand::Db(_) => "db",
                TopLevelCommand::Keys(_) => "keys",
                TopLevelCommand::Accounts(_) => "accounts",
                TopLevelCommand::Transaction(_) => "transaction",
                TopLevelCommand::Rule(_) => "rule",
                TopLevelCommand::TradingVehicle(_) => "trading-vehicle",
                TopLevelCommand::Trade(_) => "trade",
                TopLevelCommand::Distribution(_) => "distribution",
                TopLevelCommand::Report(_) => "report",
                TopLevelCommand::MarketData(_) => "market-data",
                TopLevelCommand::Grade(_) => "grade",
                TopLevelCommand::Level(_) => "level",
                TopLevelCommand::Onboarding(_) => "onboarding",
                TopLevelCommand::Policy(_) => "policy",
                TopLevelCommand::Metrics(_) => "metrics",
                TopLevelCommand::Advisor(_) => "advisor",
                TopLevelCommand::External { .. } => "external",
            };
            assert_eq!(got, expected, "top-level route mismatch for {name}");
        }
    }

    #[test]
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn parse_subcommands_cover_remaining_variants() {
        let keys = Command::new("trust").subcommand(
            Command::new("keys")
                .subcommand(Command::new("create"))
                .subcommand(Command::new("show"))
                .subcommand(Command::new("delete"))
                .subcommand(Command::new("protected-set"))
                .subcommand(Command::new("protected-show"))
                .subcommand(Command::new("protected-delete")),
        );
        for (sub, expected) in [
            ("create", "create"),
            ("show", "show"),
            ("delete", "delete"),
            ("protected-set", "protected-set"),
            ("protected-show", "protected-show"),
            ("protected-delete", "protected-delete"),
        ] {
            let m = keys.clone().get_matches_from(["trust", "keys", sub]);
            let (_, sm) = m.subcommand().expect("expected keys");
            let got = match parse_keys_subcommand(sm) {
                KeysSubcommand::Create(_) => "create",
                KeysSubcommand::Show => "show",
                KeysSubcommand::Delete(_) => "delete",
                KeysSubcommand::ProtectedSet(_) => "protected-set",
                KeysSubcommand::ProtectedShow => "protected-show",
                KeysSubcommand::ProtectedDelete(_) => "protected-delete",
            };
            assert_eq!(got, expected);
        }

        let account = Command::new("trust").subcommand(
            Command::new("account")
                .subcommand(Command::new("create"))
                .subcommand(Command::new("search"))
                .subcommand(Command::new("list"))
                .subcommand(Command::new("balance"))
                .subcommand(Command::new("transfer")),
        );
        for (sub, expected) in [
            ("create", "create"),
            ("search", "search"),
            ("list", "list"),
            ("balance", "balance"),
            ("transfer", "transfer"),
        ] {
            let m = account.clone().get_matches_from(["trust", "account", sub]);
            let (_, sm) = m.subcommand().expect("expected account");
            let got = match parse_accounts_subcommand(sm) {
                AccountsSubcommand::Create(_) => "create",
                AccountsSubcommand::Search => "search",
                AccountsSubcommand::List(_) => "list",
                AccountsSubcommand::Balance(_) => "balance",
                AccountsSubcommand::Transfer(_) => "transfer",
            };
            assert_eq!(got, expected);
        }

        let report = Command::new("trust").subcommand(
            Command::new("report")
                .subcommand(Command::new("drawdown"))
                .subcommand(Command::new("performance"))
                .subcommand(Command::new("risk"))
                .subcommand(Command::new("concentration"))
                .subcommand(Command::new("summary"))
                .subcommand(Command::new("metrics"))
                .subcommand(Command::new("attribution"))
                .subcommand(Command::new("benchmark"))
                .subcommand(Command::new("timeline")),
        );
        for (sub, expected) in [
            ("drawdown", "drawdown"),
            ("performance", "performance"),
            ("risk", "risk"),
            ("concentration", "concentration"),
            ("summary", "summary"),
            ("metrics", "metrics"),
            ("attribution", "attribution"),
            ("benchmark", "benchmark"),
            ("timeline", "timeline"),
        ] {
            let m = report.clone().get_matches_from(["trust", "report", sub]);
            let (_, sm) = m.subcommand().expect("expected report");
            let got = match parse_report_subcommand(sm) {
                ReportSubcommand::Drawdown(_) => "drawdown",
                ReportSubcommand::Performance(_) => "performance",
                ReportSubcommand::Risk(_) => "risk",
                ReportSubcommand::Concentration(_) => "concentration",
                ReportSubcommand::Summary(_) => "summary",
                ReportSubcommand::Metrics(_) => "metrics",
                ReportSubcommand::Attribution(_) => "attribution",
                ReportSubcommand::Benchmark(_) => "benchmark",
                ReportSubcommand::Timeline(_) => "timeline",
            };
            assert_eq!(got, expected);
        }

        let level = Command::new("trust").subcommand(
            Command::new("level")
                .subcommand(Command::new("triggers"))
                .subcommand(Command::new("status"))
                .subcommand(Command::new("history"))
                .subcommand(Command::new("change"))
                .subcommand(Command::new("evaluate"))
                .subcommand(Command::new("progress"))
                .subcommand(
                    Command::new("rules")
                        .subcommand(Command::new("show"))
                        .subcommand(Command::new("set")),
                ),
        );
        for (sub_path, expected) in [
            (vec!["triggers"], "triggers"),
            (vec!["status"], "status"),
            (vec!["history"], "history"),
            (vec!["change"], "change"),
            (vec!["evaluate"], "evaluate"),
            (vec!["progress"], "progress"),
            (vec!["rules", "show"], "rules-show"),
            (vec!["rules", "set"], "rules-set"),
        ] {
            let mut argv = vec!["trust", "level"];
            argv.extend(sub_path);
            let m = level.clone().get_matches_from(argv);
            let (_, sm) = m.subcommand().expect("expected level");
            let got = match parse_level_subcommand(sm) {
                LevelSubcommand::Triggers(_) => "triggers",
                LevelSubcommand::Status(_) => "status",
                LevelSubcommand::History(_) => "history",
                LevelSubcommand::Change(_) => "change",
                LevelSubcommand::Evaluate(_) => "evaluate",
                LevelSubcommand::Progress(_) => "progress",
                LevelSubcommand::RulesShow(_) => "rules-show",
                LevelSubcommand::RulesSet(_) => "rules-set",
            };
            assert_eq!(got, expected);
        }

        let onboarding = Command::new("trust").subcommand(
            Command::new("onboarding")
                .subcommand(Command::new("init"))
                .subcommand(Command::new("status")),
        );
        for (sub, expected) in [("init", "init"), ("status", "status")] {
            let m = onboarding
                .clone()
                .get_matches_from(["trust", "onboarding", sub]);
            let (_, sm) = m.subcommand().expect("expected onboarding");
            let got = match parse_onboarding_subcommand(sm) {
                OnboardingSubcommand::Init(_) => "init",
                OnboardingSubcommand::Status => "status",
            };
            assert_eq!(got, expected);
        }

        let advisor = Command::new("trust").subcommand(
            Command::new("advisor")
                .subcommand(Command::new("configure"))
                .subcommand(Command::new("check"))
                .subcommand(Command::new("status"))
                .subcommand(Command::new("history")),
        );
        for (sub, expected) in [
            ("configure", "configure"),
            ("check", "check"),
            ("status", "status"),
            ("history", "history"),
        ] {
            let m = advisor.clone().get_matches_from(["trust", "advisor", sub]);
            let (_, sm) = m.subcommand().expect("expected advisor");
            let got = match parse_advisor_subcommand(sm) {
                AdvisorSubcommand::Configure(_) => "configure",
                AdvisorSubcommand::Check(_) => "check",
                AdvisorSubcommand::Status(_) => "status",
                AdvisorSubcommand::History(_) => "history",
            };
            assert_eq!(got, expected);
        }
    }

    #[test]
    fn parse_grade_subcommand_covers_summary_variant() {
        let app = Command::new("trust").subcommand(
            Command::new("grade")
                .subcommand(Command::new("show"))
                .subcommand(Command::new("summary")),
        );
        let m = app.get_matches_from(["trust", "grade", "summary"]);
        let (_, sm) = m.subcommand().expect("expected grade");
        assert!(matches!(
            parse_grade_subcommand(sm),
            GradeSubcommand::Summary(_)
        ));
    }

    #[test]
    #[should_panic]
    fn parse_top_level_command_panics_without_subcommand() {
        let app = Command::new("trust");
        let m = app.get_matches_from(["trust"]);
        let _ = parse_top_level_command(&m);
    }

    #[test]
    #[should_panic(expected = "No subcommand provided")]
    fn parse_grade_subcommand_panics_without_nested_subcommand() {
        let app = Command::new("trust").subcommand(Command::new("grade"));
        let m = app.get_matches_from(["trust", "grade"]);
        let (_, sm) = m.subcommand().expect("expected grade");
        let _ = parse_grade_subcommand(sm);
    }

    #[test]
    #[should_panic(expected = "No subcommand provided")]
    fn parse_onboarding_subcommand_panics_without_nested_subcommand() {
        let app = Command::new("trust").subcommand(Command::new("onboarding"));
        let m = app.get_matches_from(["trust", "onboarding"]);
        let (_, sm) = m.subcommand().expect("expected onboarding");
        let _ = parse_onboarding_subcommand(sm);
    }

    #[test]
    #[should_panic(expected = "No subcommand provided")]
    fn parse_rules_subcommand_panics_without_nested_subcommand() {
        let app = Command::new("trust").subcommand(Command::new("rule"));
        let m = app.get_matches_from(["trust", "rule"]);
        let (_, sm) = m.subcommand().expect("expected rule");
        let _ = parse_rules_subcommand(sm);
    }

    #[test]
    #[should_panic(expected = "No subcommand provided")]
    fn parse_report_subcommand_panics_without_nested_subcommand() {
        let app = Command::new("trust").subcommand(Command::new("report"));
        let m = app.get_matches_from(["trust", "report"]);
        let (_, sm) = m.subcommand().expect("expected report");
        let _ = parse_report_subcommand(sm);
    }

    #[test]
    #[should_panic(expected = "No subcommand provided")]
    fn parse_market_data_subcommand_panics_without_nested_subcommand() {
        let app = Command::new("trust").subcommand(Command::new("market-data"));
        let m = app.get_matches_from(["trust", "market-data"]);
        let (_, sm) = m.subcommand().expect("expected market-data");
        let _ = parse_market_data_subcommand(sm);
    }

    #[test]
    fn parse_market_data_subcommand_covers_stream_variant() {
        let app = Command::new("trust").subcommand(
            Command::new("market-data")
                .subcommand(Command::new("snapshot"))
                .subcommand(Command::new("bars"))
                .subcommand(Command::new("stream")),
        );
        let m = app.get_matches_from(["trust", "market-data", "stream"]);
        let (_, sm) = m.subcommand().expect("expected market-data");
        assert!(matches!(
            parse_market_data_subcommand(sm),
            MarketDataSubcommand::Stream(_)
        ));
    }

    #[test]
    #[should_panic(expected = "No subcommand provided")]
    fn parse_level_rules_panics_without_rules_nested_subcommand() {
        let app = Command::new("trust")
            .subcommand(Command::new("level").subcommand(Command::new("rules")));
        let m = app.get_matches_from(["trust", "level", "rules"]);
        let (_, sm) = m.subcommand().expect("expected level");
        let _ = parse_level_subcommand(sm);
    }
}
