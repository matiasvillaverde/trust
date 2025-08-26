use clap::{Arg, Command};

pub struct DistributionCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl DistributionCommandBuilder {
    pub fn new() -> Self {
        DistributionCommandBuilder {
            command: Command::new("distribution")
                .about("Manage profit distribution rules and execution")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn configure_distribution(mut self) -> Self {
        self.subcommands.push(
            Command::new("configure")
                .about("Configure distribution rules for an account")
                .arg(
                    Arg::new("account-id")
                        .long("account-id")
                        .short('a')
                        .value_name("UUID")
                        .help("Account ID to configure distribution rules for")
                        .required(true),
                )
                .arg(
                    Arg::new("earnings")
                        .long("earnings")
                        .short('e')
                        .value_name("PERCENT")
                        .help("Percentage for earnings allocation (e.g., 40.0 for 40%)")
                        .required(true),
                )
                .arg(
                    Arg::new("tax")
                        .long("tax")
                        .short('t')
                        .value_name("PERCENT")
                        .help("Percentage for tax reserve allocation (e.g., 30.0 for 30%)")
                        .required(true),
                )
                .arg(
                    Arg::new("reinvestment")
                        .long("reinvestment")
                        .short('r')
                        .value_name("PERCENT")
                        .help("Percentage for reinvestment allocation (e.g., 30.0 for 30%)")
                        .required(true),
                )
                .arg(
                    Arg::new("threshold")
                        .long("threshold")
                        .short('m')
                        .value_name("DECIMAL")
                        .help("Minimum profit threshold for distribution")
                        .required(true),
                ),
        );
        self
    }

    pub fn execute_distribution(mut self) -> Self {
        self.subcommands.push(
            Command::new("execute")
                .about("Execute profit distribution for an account")
                .arg(
                    Arg::new("account-id")
                        .long("account-id")
                        .short('a')
                        .value_name("UUID")
                        .help("Source account ID for distribution")
                        .required(true),
                )
                .arg(
                    Arg::new("earnings-account")
                        .long("earnings-account")
                        .short('e')
                        .value_name("UUID")
                        .help("Earnings account ID")
                        .required(true),
                )
                .arg(
                    Arg::new("tax-account")
                        .long("tax-account")
                        .short('t')
                        .value_name("UUID")
                        .help("Tax reserve account ID")
                        .required(true),
                )
                .arg(
                    Arg::new("reinvestment-account")
                        .long("reinvestment-account")
                        .short('r')
                        .value_name("UUID")
                        .help("Reinvestment account ID")
                        .required(true),
                )
                .arg(
                    Arg::new("amount")
                        .long("amount")
                        .short('m')
                        .value_name("DECIMAL")
                        .help("Profit amount to distribute")
                        .required(true),
                ),
        );
        self
    }
}
