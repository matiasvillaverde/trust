use clap::{Arg, Command};

pub struct TradeCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl TradeCommandBuilder {
    pub fn new() -> Self {
        TradeCommandBuilder {
            command: Command::new("trade")
                .about("Manage trades for your account")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn create_trade(mut self) -> Self {
        self.subcommands.push(
            Command::new("create")
                .about("Create a new trade for your account")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Account name or UUID (non-interactive mode)"),
                )
                .arg(
                    Arg::new("symbol")
                        .long("symbol")
                        .value_name("SYMBOL")
                        .help("Trading symbol (non-interactive mode, e.g. AAPL)"),
                )
                .arg(
                    Arg::new("category")
                        .long("category")
                        .value_name("CATEGORY")
                        .value_parser(["long", "short"])
                        .help("Trade category (non-interactive mode)"),
                )
                .arg(
                    Arg::new("entry")
                        .long("entry")
                        .value_name("PRICE")
                        .help("Entry price (non-interactive mode)"),
                )
                .arg(
                    Arg::new("stop")
                        .long("stop")
                        .value_name("PRICE")
                        .help("Stop price (non-interactive mode)"),
                )
                .arg(
                    Arg::new("target")
                        .long("target")
                        .value_name("PRICE")
                        .help("Target price (non-interactive mode)"),
                )
                .arg(
                    Arg::new("quantity")
                        .long("quantity")
                        .value_name("QTY")
                        .help("Quantity (non-interactive mode)"),
                )
                .arg(
                    Arg::new("currency")
                        .long("currency")
                        .value_name("CURRENCY")
                        .default_value("usd")
                        .help("Currency code (usd|eur|btc) (non-interactive mode)"),
                )
                .arg(
                    Arg::new("thesis")
                        .long("thesis")
                        .value_name("TEXT")
                        .help("Trade thesis (optional, non-interactive mode)"),
                )
                .arg(
                    Arg::new("sector")
                        .long("sector")
                        .value_name("TEXT")
                        .help("Sector (optional, non-interactive mode)"),
                )
                .arg(
                    Arg::new("asset-class")
                        .long("asset-class")
                        .value_name("TEXT")
                        .help("Asset class (optional, non-interactive mode)"),
                )
                .arg(
                    Arg::new("context")
                        .long("context")
                        .value_name("TEXT")
                        .help("Trading context (optional, non-interactive mode)"),
                )
                .arg(
                    Arg::new("auto-fund")
                        .long("auto-fund")
                        .help("Fund the trade immediately after creation")
                        .action(clap::ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("auto-submit")
                        .long("auto-submit")
                        .help("Submit the trade immediately (implies --auto-fund)")
                        .action(clap::ArgAction::SetTrue),
                ),
        );
        self
    }

    pub fn fund_trade(mut self) -> Self {
        self.subcommands.push(
            Command::new("fund")
                .about("Fund a trade with your account balance")
                .arg(
                    Arg::new("trade-id")
                        .long("trade-id")
                        .value_name("UUID")
                        .help("Trade UUID (non-interactive mode)"),
                ),
        );
        self
    }

    pub fn cancel_trade(mut self) -> Self {
        self.subcommands.push(
            Command::new("cancel")
                .about(
                    "The trade balance that is not in the market will be returned to your account balance",
                )
                .arg(
                    Arg::new("trade-id")
                        .long("trade-id")
                        .value_name("UUID")
                        .help("Trade UUID (non-interactive mode)"),
                ),
        );
        self
    }

    pub fn submit_trade(mut self) -> Self {
        self.subcommands.push(
            Command::new("submit")
                .about("Submit a trade to a broker for execution. This will create an entry order in the broker's system")
                .arg(
                    Arg::new("trade-id")
                        .long("trade-id")
                        .value_name("UUID")
                        .help("Trade UUID (non-interactive mode)"),
                ),
        );
        self
    }

    pub fn sync_trade(mut self) -> Self {
        self.subcommands.push(
            Command::new("sync")
                .about("Sync a trade with the broker. This will update the trade with the broker's system")
                .arg(
                    Arg::new("trade-id")
                        .long("trade-id")
                        .value_name("UUID")
                        .help("Trade UUID (non-interactive mode)"),
                ),
        );
        self
    }

    pub fn watch_trade(mut self) -> Self {
        self.subcommands.push(
            Command::new("watch")
                .about(
                    "Watch a trade until it reaches a terminal status (polls sync + shows new executions)",
                )
                .arg(
                    Arg::new("latest")
                        .long("latest")
                        .help("Watch the most recently updated open trade without prompting")
                        .action(clap::ArgAction::SetTrue),
                ),
        );
        self
    }

    pub fn manually_fill(mut self) -> Self {
        self.subcommands.push(
            Command::new("manually-fill").about("Execute manually the filling of a trade. Meaning that the entry order was filled and we own the trading vehicle."),
        );
        self
    }

    pub fn manually_stop(mut self) -> Self {
        self.subcommands.push(
            Command::new("manually-stop").about("Execute manually the safety stop of a trade."),
        );
        self
    }

    pub fn modify_stop(mut self) -> Self {
        self.subcommands.push(
            Command::new("modify-stop").about("Modify the stop loss order of a filled trade."),
        );
        self
    }

    pub fn modify_target(mut self) -> Self {
        self.subcommands.push(
            Command::new("modify-target").about("Modify the target order of a filled trade."),
        );
        self
    }

    pub fn manually_target(mut self) -> Self {
        self.subcommands
            .push(Command::new("manually-target").about("Execute manually the target of a trade"));
        self
    }

    pub fn search_trade(mut self) -> Self {
        self.subcommands.push(
            Command::new("search")
                .about("Search trades with optional non-interactive filters")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Account ID or name (non-interactive mode)"),
                )
                .arg(
                    Arg::new("status")
                        .long("status")
                        .value_name("STATUS")
                        .help("Trade status filter (e.g. new,funded,submitted,filled,canceled)"),
                )
                .arg(
                    Arg::new("symbol")
                        .long("symbol")
                        .value_name("SYMBOL")
                        .help("Trading symbol filter (case-insensitive)"),
                )
                .arg(
                    Arg::new("from")
                        .long("from")
                        .value_name("YYYY-MM-DD")
                        .help("Start date filter (inclusive)"),
                )
                .arg(
                    Arg::new("to")
                        .long("to")
                        .value_name("YYYY-MM-DD")
                        .help("End date filter (inclusive)"),
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

    pub fn list_open(mut self) -> Self {
        self.subcommands.push(
            Command::new("list-open")
                .about("List currently open trades without interactive prompts")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Optional account ID or name"),
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

    pub fn reconcile(mut self) -> Self {
        self.subcommands.push(
            Command::new("reconcile")
                .about("Sync one or more open trades with the broker in batch mode")
                .arg(
                    Arg::new("trade-id")
                        .long("trade-id")
                        .value_name("UUID")
                        .help("Specific trade UUID to reconcile"),
                )
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Optional account ID or name when reconciling all open trades"),
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

    pub fn manually_close(mut self) -> Self {
        self.subcommands.push(
            Command::new("manually-close")
                .about("Manually close a trade with optional automatic profit distribution")
                .arg(
                    clap::Arg::new("auto-distribute")
                        .long("auto-distribute")
                        .short('d')
                        .help("Automatically distribute profits after closing the trade")
                        .action(clap::ArgAction::SetTrue),
                ),
        );
        self
    }

    pub fn size_preview(mut self) -> Self {
        self.subcommands.push(
            Command::new("size-preview")
                .about("Preview base and level-adjusted position sizes")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .short('a')
                        .value_name("ACCOUNT_ID")
                        .help("Target account ID")
                        .required(true),
                )
                .arg(
                    Arg::new("entry")
                        .long("entry")
                        .value_name("PRICE")
                        .help("Planned entry price")
                        .required(true),
                )
                .arg(
                    Arg::new("stop")
                        .long("stop")
                        .value_name("PRICE")
                        .help("Planned stop price")
                        .required(true),
                )
                .arg(
                    Arg::new("currency")
                        .long("currency")
                        .value_name("CURRENCY")
                        .help("Currency code (USD by default)")
                        .required(false)
                        .default_value("usd"),
                )
                .arg(
                    Arg::new("format")
                        .long("format")
                        .value_name("FORMAT")
                        .help("Output format")
                        .value_parser(["text", "json"])
                        .default_value("text")
                        .required(false),
                ),
        );
        self
    }
}

#[cfg(test)]
mod tests {
    use super::TradeCommandBuilder;

    fn full_trade_command() -> clap::Command {
        TradeCommandBuilder::new()
            .create_trade()
            .fund_trade()
            .cancel_trade()
            .submit_trade()
            .sync_trade()
            .watch_trade()
            .manually_fill()
            .manually_stop()
            .modify_stop()
            .modify_target()
            .manually_target()
            .search_trade()
            .list_open()
            .reconcile()
            .manually_close()
            .size_preview()
            .build()
    }

    #[test]
    fn trade_builder_registers_all_subcommands() {
        let cmd = full_trade_command();
        for name in [
            "create",
            "fund",
            "cancel",
            "submit",
            "sync",
            "watch",
            "manually-fill",
            "manually-stop",
            "modify-stop",
            "modify-target",
            "manually-target",
            "search",
            "list-open",
            "reconcile",
            "manually-close",
            "size-preview",
        ] {
            assert!(cmd.get_subcommands().any(|c| c.get_name() == name));
        }
    }

    #[test]
    fn trade_create_parses_non_interactive_fields() {
        let cmd = TradeCommandBuilder::new().create_trade().build();
        let matches = cmd
            .try_get_matches_from([
                "trade",
                "create",
                "--account",
                "paper",
                "--symbol",
                "AAPL",
                "--category",
                "long",
                "--entry",
                "100",
                "--stop",
                "95",
                "--target",
                "110",
                "--quantity",
                "10",
                "--currency",
                "usd",
                "--thesis",
                "breakout",
                "--sector",
                "tech",
                "--asset-class",
                "equity",
                "--context",
                "swing",
                "--auto-submit",
            ])
            .expect("trade create should parse");
        let sub = matches
            .subcommand_matches("create")
            .expect("create subcommand");
        assert_eq!(
            sub.get_one::<String>("symbol").map(String::as_str),
            Some("AAPL")
        );
        assert_eq!(
            sub.get_one::<String>("category").map(String::as_str),
            Some("long")
        );
        assert!(sub.get_flag("auto-submit"));
    }

    #[test]
    fn trade_size_preview_requires_account_entry_stop() {
        let cmd = TradeCommandBuilder::new().size_preview().build();
        let matches = cmd
            .try_get_matches_from([
                "trade",
                "size-preview",
                "--account",
                "acc-id",
                "--entry",
                "100",
                "--stop",
                "95",
                "--currency",
                "usd",
                "--format",
                "json",
            ])
            .expect("size-preview should parse");
        let sub = matches
            .subcommand_matches("size-preview")
            .expect("size-preview subcommand");
        assert_eq!(
            sub.get_one::<String>("account").map(String::as_str),
            Some("acc-id")
        );
        assert_eq!(
            sub.get_one::<String>("format").map(String::as_str),
            Some("json")
        );
    }

    #[test]
    fn trade_watch_and_manual_close_flags_parse() {
        let cmd = full_trade_command();
        let watch = cmd
            .clone()
            .try_get_matches_from(["trade", "watch", "--latest"])
            .expect("watch should parse");
        assert!(watch
            .subcommand_matches("watch")
            .expect("watch subcommand")
            .get_flag("latest"));

        let close = cmd
            .try_get_matches_from(["trade", "manually-close", "--auto-distribute"])
            .expect("manually-close should parse");
        assert!(close
            .subcommand_matches("manually-close")
            .expect("manually-close subcommand")
            .get_flag("auto-distribute"));
    }

    #[test]
    fn trade_search_list_open_and_reconcile_parse_filters() {
        let search = TradeCommandBuilder::new().search_trade().build();
        let search_matches = search
            .try_get_matches_from([
                "trade",
                "search",
                "--account",
                "acc-1",
                "--status",
                "submitted",
                "--symbol",
                "AAPL",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
                "--format",
                "json",
            ])
            .expect("search should parse");
        let search_sub = search_matches
            .subcommand_matches("search")
            .expect("search subcommand");
        assert_eq!(
            search_sub.get_one::<String>("status").map(String::as_str),
            Some("submitted")
        );
        assert_eq!(
            search_sub.get_one::<String>("format").map(String::as_str),
            Some("json")
        );

        let list_open = TradeCommandBuilder::new().list_open().build();
        let list_open_matches = list_open
            .try_get_matches_from(["trade", "list-open", "--account", "acc-2"])
            .expect("list-open should parse");
        assert!(list_open_matches.subcommand_matches("list-open").is_some());

        let reconcile = TradeCommandBuilder::new().reconcile().build();
        let reconcile_matches = reconcile
            .try_get_matches_from([
                "trade",
                "reconcile",
                "--trade-id",
                "00000000-0000-0000-0000-000000000000",
                "--format",
                "json",
            ])
            .expect("reconcile should parse");
        let reconcile_sub = reconcile_matches
            .subcommand_matches("reconcile")
            .expect("reconcile subcommand");
        assert_eq!(
            reconcile_sub
                .get_one::<String>("format")
                .map(String::as_str),
            Some("json")
        );
    }
}
