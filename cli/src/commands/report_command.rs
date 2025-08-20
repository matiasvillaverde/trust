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
}

impl Default for ReportCommandBuilder {
    fn default() -> Self {
        Self::new()
    }
}
