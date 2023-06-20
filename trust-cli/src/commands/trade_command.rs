use clap::Command;

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
        self.subcommands
            .push(Command::new("create").about("Create a new trade for your account"));
        self
    }

    pub fn fund_trade(mut self) -> Self {
        self.subcommands
            .push(Command::new("fund").about("Fund a trade with your account balance"));
        self
    }

    pub fn submit_trade(mut self) -> Self {
        self.subcommands.push(
            Command::new("submit")
                .about("Submit a trade to a broker for execution. This will create an entry order in the broker's system"),
        );
        self
    }

    pub fn sync_trade(mut self) -> Self {
        self.subcommands.push(Command::new("sync").about(
            "Sync a trade with the broker. This will update the trade with the broker's system",
        ));
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

    pub fn manually_target(mut self) -> Self {
        self.subcommands
            .push(Command::new("manually-target").about("Execute manually the target of a trade"));
        self
    }

    pub fn search_trade(mut self) -> Self {
        self.subcommands
            .push(Command::new("search").about("Search Trades for an account"));
        self
    }

    pub fn manually_close(mut self) -> Self {
        self.subcommands
            .push(Command::new("manually-close").about("Manually close a trade"));
        self
    }
}
