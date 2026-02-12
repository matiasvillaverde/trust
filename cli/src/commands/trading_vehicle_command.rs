use clap::{Arg, ArgAction, Command};

pub struct TradingVehicleCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl TradingVehicleCommandBuilder {
    pub fn new() -> Self {
        TradingVehicleCommandBuilder {
            command: Command::new("trading-vehicle")
                .about("Manage Trading Vehicles like stocks, crypto, etc.")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn create_trading_vehicle(mut self) -> Self {
        self.subcommands.push(
            Command::new("create")
                .about("Create a new trading vehicle")
                .arg(
                    Arg::new("from-alpaca")
                        .long("from-alpaca")
                        .action(ArgAction::SetTrue)
                        .help("Fetch symbol metadata from Alpaca instead of prompting manually"),
                )
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT_NAME")
                        .help("Account name used to resolve Alpaca keys"),
                )
                .arg(
                    Arg::new("symbol")
                        .long("symbol")
                        .value_name("SYMBOL")
                        .help("Trading symbol to fetch from broker metadata"),
                ),
        );
        self
    }

    pub fn search_trading_vehicle(mut self) -> Self {
        self.subcommands.push(
            Command::new("search").about("Search trading vehicles by symbol, ISIN or broker"),
        );
        self
    }
}
