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
        let cmd = MetricsCommandBuilder::new().advanced().compare().build();
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "advanced"));
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "compare"));
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
    fn metrics_default_matches_new() {
        let from_default = MetricsCommandBuilder::default().advanced().build();
        let from_new = MetricsCommandBuilder::new().advanced().build();
        assert_eq!(from_default.get_name(), from_new.get_name());
    }
}
