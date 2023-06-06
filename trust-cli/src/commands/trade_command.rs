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

    pub fn fill_trade(mut self) -> Self {
        self.subcommands.push(
            Command::new("fill").about("Execute manually the filling of a trade. Meaning that the entry order was filled and we own the trading vehicle."),
        );
        self
    }

    pub fn stop_trade(mut self) -> Self {
        self.subcommands
            .push(Command::new("stop").about("Execute manually the safety stop of a trade"));
        self
    }

    pub fn target_trade(mut self) -> Self {
        self.subcommands
            .push(Command::new("target").about("Execute manually the target of a trade"));
        self
    }
}
