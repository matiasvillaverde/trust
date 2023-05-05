use clap::Command;

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
        self.subcommands
            .push(Command::new("create").about("Create a new trading vehicle"));
        self
    }

    pub fn search_trading_vehicle(mut self) -> Self {
        self.subcommands.push(
            Command::new("search").about("Search trading vehicles by symbol, ISIN or broker"),
        );
        self
    }
}
