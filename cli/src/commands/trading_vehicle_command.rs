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
                    Arg::new("category")
                        .long("category")
                        .value_name("CATEGORY")
                        .help("Trading vehicle category for broker metadata lookup (stock|etf|bond|crypto|fiat)"),
                )
                .arg(
                    Arg::new("face-value")
                        .long("face-value")
                        .value_name("AMOUNT")
                        .help("Bond face/par value per unit"),
                )
                .arg(
                    Arg::new("coupon-rate")
                        .long("coupon-rate")
                        .value_name("PERCENT")
                        .help("Bond annual coupon rate as a percentage, e.g. 4.625"),
                )
                .arg(
                    Arg::new("maturity-date")
                        .long("maturity-date")
                        .value_name("YYYY-MM-DD")
                        .help("Bond maturity date"),
                )
                .arg(
                    Arg::new("coupon-frequency")
                        .long("coupon-frequency")
                        .value_name("PAYMENTS_PER_YEAR")
                        .help("Bond coupon payments per year")
                        .value_parser(clap::value_parser!(u16)),
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
            Command::new("search")
                .about("Search trading vehicles by symbol, ISIN, category or broker")
                .arg(
                    Arg::new("all")
                        .long("all")
                        .action(ArgAction::SetTrue)
                        .help("List all trading vehicles without opening the interactive picker"),
                )
                .arg(
                    Arg::new("category")
                        .long("category")
                        .value_name("CATEGORY")
                        .help("Filter by category (stock|etf|bond|crypto|fiat)"),
                )
                .arg(
                    Arg::new("broker")
                        .long("broker")
                        .value_name("BROKER")
                        .help("Filter by broker"),
                )
                .arg(
                    Arg::new("symbol")
                        .long("symbol")
                        .value_name("SYMBOL")
                        .help("Filter by symbol substring"),
                )
                .arg(
                    Arg::new("isin")
                        .long("isin")
                        .value_name("ISIN")
                        .help("Filter by ISIN substring"),
                )
                .arg(
                    Arg::new("missing-bond-terms")
                        .long("missing-bond-terms")
                        .action(ArgAction::SetTrue)
                        .help("Show bonds missing at least one fixed-income term"),
                )
                .arg(
                    Arg::new("format")
                        .long("format")
                        .value_name("FORMAT")
                        .help("Output format")
                        .value_parser(["text", "json"])
                        .default_value("text"),
                ),
        );
        self
    }

    pub fn update_bond_terms(mut self) -> Self {
        self.subcommands.push(
            Command::new("update-bond-terms")
                .about("Update stored fixed-income terms for an existing bond")
                .arg(
                    Arg::new("symbol")
                        .long("symbol")
                        .value_name("SYMBOL")
                        .help("Bond symbol to update")
                        .required(true),
                )
                .arg(
                    Arg::new("broker")
                        .long("broker")
                        .value_name("BROKER")
                        .help("Broker name for the stored bond")
                        .required(true),
                )
                .arg(
                    Arg::new("face-value")
                        .long("face-value")
                        .value_name("AMOUNT")
                        .help("Bond face/par value per unit"),
                )
                .arg(
                    Arg::new("coupon-rate")
                        .long("coupon-rate")
                        .value_name("PERCENT")
                        .help("Bond annual coupon rate as a percentage, e.g. 4.625"),
                )
                .arg(
                    Arg::new("maturity-date")
                        .long("maturity-date")
                        .value_name("YYYY-MM-DD")
                        .help("Bond maturity date"),
                )
                .arg(
                    Arg::new("coupon-frequency")
                        .long("coupon-frequency")
                        .value_name("PAYMENTS_PER_YEAR")
                        .help("Bond coupon payments per year")
                        .value_parser(clap::value_parser!(u16)),
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

    pub fn stats(mut self) -> Self {
        self.subcommands.push(
            Command::new("stats")
                .about("Show security inventory statistics by category, broker, and bond terms")
                .arg(
                    Arg::new("format")
                        .long("format")
                        .value_name("FORMAT")
                        .help("Output format")
                        .value_parser(["text", "json"])
                        .default_value("text"),
                ),
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
            .update_bond_terms()
            .search_trading_vehicle()
            .stats()
            .build();
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "create"));
        assert!(cmd
            .get_subcommands()
            .any(|c| c.get_name() == "update-bond-terms"));
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "search"));
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "stats"));
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
                "--category",
                "etf",
                "--face-value",
                "1000",
                "--coupon-rate",
                "4.625",
                "--maturity-date",
                "2034-05-15",
                "--coupon-frequency",
                "2",
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
            sub.get_one::<String>("category").map(String::as_str),
            Some("etf")
        );
        assert_eq!(
            sub.get_one::<String>("face-value").map(String::as_str),
            Some("1000")
        );
        assert_eq!(sub.get_one::<u16>("coupon-frequency"), Some(&2));
        assert_eq!(
            sub.get_one::<String>("confirm-protected")
                .map(String::as_str),
            Some("keyword")
        );
    }

    #[test]
    fn trading_vehicle_update_bond_terms_parses_inputs() {
        let cmd = TradingVehicleCommandBuilder::new()
            .update_bond_terms()
            .build();
        let matches = cmd
            .try_get_matches_from([
                "trading-vehicle",
                "update-bond-terms",
                "--symbol",
                "9128285M8",
                "--broker",
                "ibkr",
                "--face-value",
                "1000",
                "--coupon-rate",
                "4.625",
                "--maturity-date",
                "2034-05-15",
                "--coupon-frequency",
                "2",
                "--confirm-protected",
                "keyword",
            ])
            .expect("trading-vehicle update-bond-terms should parse");
        let sub = matches
            .subcommand_matches("update-bond-terms")
            .expect("update-bond-terms subcommand");
        assert_eq!(
            sub.get_one::<String>("symbol").map(String::as_str),
            Some("9128285M8")
        );
        assert_eq!(
            sub.get_one::<String>("broker").map(String::as_str),
            Some("ibkr")
        );
        assert_eq!(sub.get_one::<u16>("coupon-frequency"), Some(&2));
    }

    #[test]
    fn trading_vehicle_search_parses_non_interactive_filters() {
        let cmd = TradingVehicleCommandBuilder::new()
            .search_trading_vehicle()
            .build();
        let matches = cmd
            .try_get_matches_from([
                "trading-vehicle",
                "search",
                "--all",
                "--category",
                "bond",
                "--broker",
                "ibkr",
                "--symbol",
                "912",
                "--isin",
                "US",
                "--missing-bond-terms",
                "--format",
                "json",
            ])
            .expect("trading-vehicle search should parse");
        let sub = matches
            .subcommand_matches("search")
            .expect("search subcommand");
        assert!(sub.get_flag("all"));
        assert_eq!(
            sub.get_one::<String>("category").map(String::as_str),
            Some("bond")
        );
        assert_eq!(
            sub.get_one::<String>("broker").map(String::as_str),
            Some("ibkr")
        );
        assert!(sub.get_flag("missing-bond-terms"));
        assert_eq!(
            sub.get_one::<String>("format").map(String::as_str),
            Some("json")
        );
    }

    #[test]
    fn trading_vehicle_stats_parses_format() {
        let cmd = TradingVehicleCommandBuilder::new().stats().build();
        let matches = cmd
            .try_get_matches_from(["trading-vehicle", "stats", "--format", "json"])
            .expect("trading-vehicle stats should parse");
        let sub = matches
            .subcommand_matches("stats")
            .expect("stats subcommand");
        assert_eq!(
            sub.get_one::<String>("format").map(String::as_str),
            Some("json")
        );
    }
}
