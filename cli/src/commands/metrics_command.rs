use clap::{Arg, Command};

pub struct MetricsCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl MetricsCommandBuilder {
    pub fn new() -> Self {
        MetricsCommandBuilder {
            command: Command::new("metrics")
                .about("Advanced financial metrics and analysis tools")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn advanced(mut self) -> Self {
        self.subcommands.push(
            Command::new("advanced")
                .about("Display comprehensive advanced financial metrics including risk-adjusted ratios")
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
                        .help("Filter trades from the last N days (default: 90)")
                        .value_parser(clap::value_parser!(u32))
                        .required(false),
                )
                .arg(
                    Arg::new("risk-free-rate")
                        .long("risk-free-rate")
                        .value_name("RATE")
                        .help("Risk-free rate for Sharpe/Sortino calculations (default: 0.05)")
                        .value_parser(clap::value_parser!(f64))
                        .required(false),
                )
                .arg(
                    Arg::new("export")
                        .long("export")
                        .value_name("FORMAT")
                        .help("Export metrics to file (json, csv)")
                        .value_parser(["json", "csv"])
                        .required(false),
                )
                .arg(
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .value_name("FILE")
                        .help("Output file path (default: metrics.{format})")
                        .required(false),
                ),
        );
        self
    }

    pub fn compare(mut self) -> Self {
        self.subcommands.push(
            Command::new("compare")
                .about("Compare performance across time periods")
                .arg(
                    Arg::new("period1")
                        .long("period1")
                        .value_name("PERIOD1")
                        .help("First period to compare (e.g., 'last-30-days')")
                        .required(true),
                )
                .arg(
                    Arg::new("period2")
                        .long("period2")
                        .value_name("PERIOD2")
                        .help("Second period to compare (e.g., 'previous-30-days')")
                        .required(true),
                )
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT_ID")
                        .help("Filter by specific account ID")
                        .required(false),
                )
                .arg(
                    Arg::new("format")
                        .long("format")
                        .value_name("FORMAT")
                        .help("Output format")
                        .value_parser(["text", "json"])
                        .default_value("text")
                        .required(false),
                )
                .arg(
                    Arg::new("export")
                        .long("export")
                        .value_name("FORMAT")
                        .help("Export comparison to file (json, csv)")
                        .value_parser(["json", "csv"])
                        .required(false),
                )
                .arg(
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .value_name("FILE")
                        .help("Output file path for export (default: metrics-compare.{format})")
                        .required(false),
                ),
        );
        self
    }

    pub fn bond(mut self) -> Self {
        let command =
            Command::new("bond").about("Calculate fixed-income bond income and yield metrics");
        let command = Self::with_bond_lookup_args(command);
        let command = Self::with_bond_analytics_args(command);
        let command = Self::with_bond_accrued_interest_args(command);
        self.subcommands.push(Self::with_report_format_arg(command));
        self
    }

    fn with_bond_lookup_args(command: Command) -> Command {
        command
            .arg(
                Arg::new("symbol")
                    .long("symbol")
                    .value_name("SYMBOL")
                    .help("Bond symbol to load stored fixed-income terms from"),
            )
            .arg(
                Arg::new("broker")
                    .long("broker")
                    .value_name("BROKER")
                    .help("Broker for --symbol lookup"),
            )
    }

    fn with_bond_analytics_args(command: Command) -> Command {
        command
            .arg(
                Arg::new("face-value")
                    .long("face-value")
                    .value_name("AMOUNT")
                    .help("Bond face/par value per unit"),
            )
            .arg(
                Arg::new("market-price")
                    .long("market-price")
                    .value_name("AMOUNT")
                    .help("Current clean market price per unit")
                    .required(true),
            )
            .arg(
                Arg::new("coupon-rate")
                    .long("coupon-rate")
                    .value_name("PERCENT")
                    .help("Annual coupon rate as a percentage, e.g. 4.5"),
            )
            .arg(
                Arg::new("quantity")
                    .long("quantity")
                    .value_name("UNITS")
                    .help("Number of bond units")
                    .value_parser(clap::value_parser!(i64))
                    .required(true),
            )
            .arg(
                Arg::new("years-to-maturity")
                    .long("years-to-maturity")
                    .value_name("YEARS")
                    .help(
                        "Years remaining until maturity; optional when stored bond maturity is available",
                    ),
            )
    }

    fn with_bond_accrued_interest_args(command: Command) -> Command {
        command
            .arg(
                Arg::new("coupon-frequency")
                    .long("coupon-frequency")
                    .value_name("PAYMENTS_PER_YEAR")
                    .help("Coupon payments per year for accrued-interest calculations")
                    .value_parser(clap::value_parser!(u16)),
            )
            .arg(
                Arg::new("settlement-date")
                    .long("settlement-date")
                    .value_name("YYYY-MM-DD")
                    .help("Settlement date for accrued-interest calculations"),
            )
            .arg(
                Arg::new("last-coupon-date")
                    .long("last-coupon-date")
                    .value_name("YYYY-MM-DD")
                    .help("Previous coupon date for accrued-interest calculations"),
            )
            .arg(
                Arg::new("next-coupon-date")
                    .long("next-coupon-date")
                    .value_name("YYYY-MM-DD")
                    .help("Next coupon date for accrued-interest calculations"),
            )
            .arg(
                Arg::new("day-count")
                    .long("day-count")
                    .value_name("BASIS")
                    .help("Day-count basis for accrued interest")
                    .value_parser(["actual-actual", "actual-360", "actual-365"]),
            )
    }

    fn with_report_format_arg(command: Command) -> Command {
        command.arg(
            Arg::new("format")
                .long("format")
                .value_name("FORMAT")
                .help("Output format")
                .value_parser(["text", "json"])
                .default_value("text")
                .required(false),
        )
    }
}

impl Default for MetricsCommandBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::MetricsCommandBuilder;

    #[test]
    fn metrics_builder_registers_subcommands() {
        let cmd = MetricsCommandBuilder::new()
            .advanced()
            .compare()
            .bond()
            .build();
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "advanced"));
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "compare"));
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "bond"));
    }

    #[test]
    fn metrics_advanced_parses_all_options() {
        let cmd = MetricsCommandBuilder::new().advanced().build();
        let matches = cmd
            .try_get_matches_from([
                "metrics",
                "advanced",
                "--account",
                "acc-1",
                "--days",
                "90",
                "--risk-free-rate",
                "0.03",
                "--export",
                "json",
                "--output",
                "metrics.json",
            ])
            .expect("metrics advanced should parse");
        let advanced = matches
            .subcommand_matches("advanced")
            .expect("advanced subcommand");
        assert_eq!(
            advanced.get_one::<String>("account").map(String::as_str),
            Some("acc-1")
        );
        assert_eq!(advanced.get_one::<u32>("days"), Some(&90));
        assert_eq!(advanced.get_one::<f64>("risk-free-rate"), Some(&0.03_f64));
        assert_eq!(
            advanced.get_one::<String>("export").map(String::as_str),
            Some("json")
        );
    }

    #[test]
    fn metrics_compare_requires_two_periods() {
        let cmd = MetricsCommandBuilder::new().compare().build();
        let matches = cmd
            .try_get_matches_from([
                "metrics",
                "compare",
                "--period1",
                "last-30-days",
                "--period2",
                "previous-30-days",
                "--format",
                "json",
                "--export",
                "csv",
            ])
            .expect("metrics compare should parse");
        let compare = matches
            .subcommand_matches("compare")
            .expect("compare subcommand");
        assert_eq!(
            compare.get_one::<String>("period1").map(String::as_str),
            Some("last-30-days")
        );
        assert_eq!(
            compare.get_one::<String>("period2").map(String::as_str),
            Some("previous-30-days")
        );
        assert_eq!(
            compare.get_one::<String>("format").map(String::as_str),
            Some("json")
        );
    }

    #[test]
    fn metrics_compare_parses_optional_account_and_output() {
        let cmd = MetricsCommandBuilder::new().compare().build();
        let matches = cmd
            .try_get_matches_from([
                "metrics",
                "compare",
                "--period1",
                "last-7-days",
                "--period2",
                "previous-7-days",
                "--account",
                "acc-2",
                "--output",
                "compare.json",
            ])
            .expect("metrics compare should parse with optional args");
        let compare = matches
            .subcommand_matches("compare")
            .expect("compare subcommand");
        assert_eq!(
            compare.get_one::<String>("account").map(String::as_str),
            Some("acc-2")
        );
        assert_eq!(
            compare.get_one::<String>("output").map(String::as_str),
            Some("compare.json")
        );
    }

    #[test]
    fn metrics_bond_parses_required_inputs() {
        let cmd = MetricsCommandBuilder::new().bond().build();
        let matches = cmd
            .try_get_matches_from([
                "metrics",
                "bond",
                "--face-value",
                "1000",
                "--market-price",
                "950",
                "--coupon-rate",
                "4",
                "--quantity",
                "3",
                "--years-to-maturity",
                "5",
                "--coupon-frequency",
                "2",
                "--settlement-date",
                "2026-04-01",
                "--last-coupon-date",
                "2026-01-01",
                "--next-coupon-date",
                "2026-07-01",
                "--day-count",
                "actual-360",
                "--format",
                "json",
            ])
            .expect("metrics bond should parse");
        let bond = matches.subcommand_matches("bond").expect("bond subcommand");
        assert_eq!(
            bond.get_one::<String>("face-value").map(String::as_str),
            Some("1000")
        );
        assert_eq!(bond.get_one::<i64>("quantity"), Some(&3));
        assert_eq!(
            bond.get_one::<String>("format").map(String::as_str),
            Some("json")
        );
        assert_eq!(bond.get_one::<u16>("coupon-frequency"), Some(&2));
        assert_eq!(
            bond.get_one::<String>("day-count").map(String::as_str),
            Some("actual-360")
        );
    }

    #[test]
    fn metrics_bond_parses_stored_vehicle_lookup_inputs() {
        let cmd = MetricsCommandBuilder::new().bond().build();
        let matches = cmd
            .try_get_matches_from([
                "metrics",
                "bond",
                "--symbol",
                "9128285M8",
                "--broker",
                "ibkr",
                "--market-price",
                "997.50",
                "--quantity",
                "5",
            ])
            .expect("metrics bond should parse stored vehicle inputs");
        let bond = matches.subcommand_matches("bond").expect("bond subcommand");
        assert_eq!(
            bond.get_one::<String>("symbol").map(String::as_str),
            Some("9128285M8")
        );
        assert_eq!(
            bond.get_one::<String>("broker").map(String::as_str),
            Some("ibkr")
        );
        assert_eq!(bond.get_one::<i64>("quantity"), Some(&5));
    }

    #[test]
    fn metrics_default_matches_new() {
        let from_default = MetricsCommandBuilder::default().advanced().build();
        let from_new = MetricsCommandBuilder::new().advanced().build();
        assert_eq!(from_default.get_name(), from_new.get_name());
    }
}
