use clap::{Arg, Command};

pub struct ReportCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl ReportCommandBuilder {
    pub fn new() -> Self {
        ReportCommandBuilder {
            command: Command::new("report")
                .about("Generate trading performance reports")
                .arg(
                    Arg::new("format")
                        .long("format")
                        .value_name("FORMAT")
                        .help("Output format")
                        .value_parser(["text", "json"])
                        .default_value("text")
                        .global(true),
                )
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn performance(mut self) -> Self {
        self.subcommands.push(
            Command::new("performance")
                .about("Display trading performance statistics for closed trades")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT_ID")
                        .help("Filter by specific account ID")
                        .required(false),
                )
                .arg(
                    Arg::new("days")
                        .long("days")
                        .value_name("DAYS")
                        .help("Filter trades from the last N days")
                        .value_parser(clap::value_parser!(u32))
                        .required(false),
                ),
        );
        self
    }

    pub fn drawdown(mut self) -> Self {
        self.subcommands.push(
            Command::new("drawdown")
                .about("Display realized P&L drawdown analysis based on closed trades")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT_ID")
                        .help("Filter by specific account ID")
                        .required(false),
                ),
        );
        self
    }

    pub fn risk(mut self) -> Self {
        self.subcommands.push(
            Command::new("risk")
                .about("Display current capital at risk from open positions")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT_ID")
                        .help("Filter by specific account ID")
                        .required(false),
                ),
        );
        self
    }

    pub fn concentration(mut self) -> Self {
        self.subcommands.push(
            Command::new("concentration")
                .about("Display portfolio concentration analysis by sector and asset class")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT_ID")
                        .help("Filter by specific account ID")
                        .required(false),
                )
                .arg(
                    Arg::new("open-only")
                        .long("open-only")
                        .help("Show only currently open positions")
                        .action(clap::ArgAction::SetTrue)
                        .required(false),
                ),
        );
        self
    }

    pub fn summary(mut self) -> Self {
        self.subcommands.push(
            Command::new("summary")
                .about("Display comprehensive trading summary with all key metrics")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT_ID")
                        .help("Filter by specific account ID")
                        .required(false),
                ),
        );
        self
    }

    pub fn metrics(mut self) -> Self {
        self.subcommands.push(
            Command::new("metrics")
                .about("Display advanced financial metrics (profit factor, expectancy, etc.)")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT_ID")
                        .help("Filter by specific account ID")
                        .required(false),
                )
                .arg(
                    Arg::new("days")
                        .long("days")
                        .value_name("DAYS")
                        .help("Filter trades from the last N days")
                        .value_parser(clap::value_parser!(u32))
                        .required(false),
                ),
        );
        self
    }

    pub fn attribution(mut self) -> Self {
        self.subcommands.push(
            Command::new("attribution")
                .about("Attribution report grouped by symbol, sector, or asset-class")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT_ID")
                        .help("Account ID")
                        .required(true),
                )
                .arg(
                    Arg::new("by")
                        .long("by")
                        .value_name("DIMENSION")
                        .help("Attribution dimension")
                        .value_parser(["symbol", "sector", "asset-class"])
                        .required(true),
                )
                .arg(
                    Arg::new("from")
                        .long("from")
                        .value_name("YYYY-MM-DD")
                        .help("Start date, inclusive")
                        .required(true),
                )
                .arg(
                    Arg::new("to")
                        .long("to")
                        .value_name("YYYY-MM-DD")
                        .help("End date, inclusive")
                        .required(true),
                ),
        );
        self
    }

    pub fn benchmark(mut self) -> Self {
        self.subcommands.push(
            Command::new("benchmark")
                .about("Compare closed-trade return against a benchmark symbol")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT_ID")
                        .help("Account ID")
                        .required(true),
                )
                .arg(
                    Arg::new("benchmark")
                        .long("benchmark")
                        .value_name("SYMBOL")
                        .help("Benchmark symbol, e.g. SPY")
                        .required(true),
                )
                .arg(
                    Arg::new("from")
                        .long("from")
                        .value_name("YYYY-MM-DD")
                        .help("Start date, inclusive")
                        .required(true),
                )
                .arg(
                    Arg::new("to")
                        .long("to")
                        .value_name("YYYY-MM-DD")
                        .help("End date, inclusive")
                        .required(true),
                ),
        );
        self
    }

    pub fn timeline(mut self) -> Self {
        self.subcommands.push(
            Command::new("timeline")
                .about("Timeline of account cashflow by day/week/month buckets")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT_ID")
                        .help("Account ID")
                        .required(true),
                )
                .arg(
                    Arg::new("granularity")
                        .long("granularity")
                        .value_name("BUCKET")
                        .help("Bucket granularity")
                        .value_parser(["day", "week", "month"])
                        .required(true),
                )
                .arg(
                    Arg::new("from")
                        .long("from")
                        .value_name("YYYY-MM-DD")
                        .help("Start date, inclusive")
                        .required(true),
                )
                .arg(
                    Arg::new("to")
                        .long("to")
                        .value_name("YYYY-MM-DD")
                        .help("End date, inclusive")
                        .required(true),
                ),
        );
        self
    }
}

impl Default for ReportCommandBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::ReportCommandBuilder;

    #[test]
    fn report_builder_registers_all_subcommands() {
        let cmd = ReportCommandBuilder::new()
            .performance()
            .drawdown()
            .risk()
            .concentration()
            .summary()
            .metrics()
            .attribution()
            .benchmark()
            .timeline()
            .build();
        for name in [
            "performance",
            "drawdown",
            "risk",
            "concentration",
            "summary",
            "metrics",
            "attribution",
            "benchmark",
            "timeline",
        ] {
            assert!(cmd.get_subcommands().any(|c| c.get_name() == name));
        }
    }

    #[test]
    fn report_performance_parses_days_and_format() {
        let cmd = ReportCommandBuilder::new().performance().build();
        let matches = cmd
            .try_get_matches_from([
                "report",
                "--format",
                "json",
                "performance",
                "--account",
                "acc-1",
                "--days",
                "30",
            ])
            .expect("report performance should parse");
        assert_eq!(
            matches.get_one::<String>("format").map(String::as_str),
            Some("json")
        );
        let sub = matches
            .subcommand_matches("performance")
            .expect("performance subcommand");
        assert_eq!(
            sub.get_one::<String>("account").map(String::as_str),
            Some("acc-1")
        );
        assert_eq!(sub.get_one::<u32>("days"), Some(&30));
    }

    #[test]
    fn report_concentration_parses_open_only_flag() {
        let cmd = ReportCommandBuilder::new().concentration().build();
        let matches = cmd
            .try_get_matches_from(["report", "concentration", "--open-only"])
            .expect("report concentration should parse");
        let sub = matches
            .subcommand_matches("concentration")
            .expect("concentration subcommand");
        assert!(sub.get_flag("open-only"));
    }

    #[test]
    fn report_other_subcommands_parse() {
        let drawdown = ReportCommandBuilder::new().drawdown().build();
        let drawdown_matches = drawdown
            .try_get_matches_from(["report", "drawdown", "--account", "acc-1"])
            .expect("drawdown should parse");
        let drawdown_sub = drawdown_matches
            .subcommand_matches("drawdown")
            .expect("drawdown subcommand");
        assert_eq!(
            drawdown_sub
                .get_one::<String>("account")
                .map(String::as_str),
            Some("acc-1")
        );

        let risk = ReportCommandBuilder::new().risk().build();
        let risk_matches = risk
            .try_get_matches_from(["report", "risk", "--account", "acc-2"])
            .expect("risk should parse");
        let risk_sub = risk_matches
            .subcommand_matches("risk")
            .expect("risk subcommand");
        assert_eq!(
            risk_sub.get_one::<String>("account").map(String::as_str),
            Some("acc-2")
        );

        let summary = ReportCommandBuilder::new().summary().build();
        let summary_matches = summary
            .try_get_matches_from(["report", "summary", "--account", "acc-3"])
            .expect("summary should parse");
        let summary_sub = summary_matches
            .subcommand_matches("summary")
            .expect("summary subcommand");
        assert_eq!(
            summary_sub.get_one::<String>("account").map(String::as_str),
            Some("acc-3")
        );

        let metrics = ReportCommandBuilder::new().metrics().build();
        let metrics_matches = metrics
            .try_get_matches_from(["report", "metrics", "--account", "acc-4", "--days", "14"])
            .expect("metrics should parse");
        let metrics_sub = metrics_matches
            .subcommand_matches("metrics")
            .expect("metrics subcommand");
        assert_eq!(
            metrics_sub.get_one::<String>("account").map(String::as_str),
            Some("acc-4")
        );
        assert_eq!(metrics_sub.get_one::<u32>("days"), Some(&14));
    }

    #[test]
    fn report_default_matches_new() {
        let from_default = ReportCommandBuilder::default().summary().build();
        let from_new = ReportCommandBuilder::new().summary().build();
        assert_eq!(from_default.get_name(), from_new.get_name());
    }

    #[test]
    fn report_attribution_benchmark_timeline_parse() {
        let attribution = ReportCommandBuilder::new().attribution().build();
        let attribution_matches = attribution
            .try_get_matches_from([
                "report",
                "attribution",
                "--account",
                "acc-1",
                "--by",
                "symbol",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ])
            .expect("attribution should parse");
        let attribution_sub = attribution_matches
            .subcommand_matches("attribution")
            .expect("attribution subcommand");
        assert_eq!(
            attribution_sub
                .get_one::<String>("account")
                .map(String::as_str),
            Some("acc-1")
        );
        assert_eq!(
            attribution_sub.get_one::<String>("by").map(String::as_str),
            Some("symbol")
        );

        let benchmark = ReportCommandBuilder::new().benchmark().build();
        let benchmark_matches = benchmark
            .try_get_matches_from([
                "report",
                "benchmark",
                "--account",
                "acc-2",
                "--benchmark",
                "SPY",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ])
            .expect("benchmark should parse");
        let benchmark_sub = benchmark_matches
            .subcommand_matches("benchmark")
            .expect("benchmark subcommand");
        assert_eq!(
            benchmark_sub
                .get_one::<String>("benchmark")
                .map(String::as_str),
            Some("SPY")
        );

        let timeline = ReportCommandBuilder::new().timeline().build();
        let timeline_matches = timeline
            .try_get_matches_from([
                "report",
                "timeline",
                "--account",
                "acc-3",
                "--granularity",
                "week",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ])
            .expect("timeline should parse");
        let timeline_sub = timeline_matches
            .subcommand_matches("timeline")
            .expect("timeline subcommand");
        assert_eq!(
            timeline_sub
                .get_one::<String>("granularity")
                .map(String::as_str),
            Some("week")
        );
    }
}
