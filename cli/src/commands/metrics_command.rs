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
