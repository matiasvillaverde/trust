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
                    Arg::new("from-broker")
                        .long("from-broker")
                        .value_name("BROKER")
                        .help("Fetch symbol metadata from a broker (alpaca|ibkr) instead of prompting manually"),
                )
                .arg(
                    Arg::new("from-alpaca")
                        .long("from-alpaca")
                        .action(ArgAction::SetTrue)
                        .help("Deprecated alias for --from-broker alpaca"),
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
                )
                .arg(
                    Arg::new("confirm-protected")
                        .long("confirm-protected")
                        .value_name("KEYWORD")
                        .help("Protected mutation keyword")
                        .required(false),
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

#[cfg(test)]
mod tests {
    use super::TradingVehicleCommandBuilder;

    #[test]
    fn trading_vehicle_builder_registers_subcommands() {
        let cmd = TradingVehicleCommandBuilder::new()
            .create_trading_vehicle()
            .search_trading_vehicle()
            .build();
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "create"));
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "search"));
    }

    #[test]
    fn trading_vehicle_create_parses_broker_import_and_symbol_options() {
        let cmd = TradingVehicleCommandBuilder::new()
            .create_trading_vehicle()
            .build();
        let matches = cmd
            .try_get_matches_from([
                "trading-vehicle",
                "create",
                "--from-broker",
                "ibkr",
                "--account",
                "paper",
                "--symbol",
                "AAPL",
                "--confirm-protected",
                "keyword",
            ])
            .expect("trading-vehicle create should parse");
        let sub = matches
            .subcommand_matches("create")
            .expect("create subcommand");
        assert_eq!(
            sub.get_one::<String>("from-broker").map(String::as_str),
            Some("ibkr")
        );
        assert_eq!(
            sub.get_one::<String>("account").map(String::as_str),
            Some("paper")
        );
        assert_eq!(
            sub.get_one::<String>("symbol").map(String::as_str),
            Some("AAPL")
        );
        assert_eq!(
            sub.get_one::<String>("confirm-protected")
                .map(String::as_str),
            Some("keyword")
        );
    }
}
