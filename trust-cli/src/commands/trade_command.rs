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

    pub fn approve_trade(mut self) -> Self {
        self.subcommands
            .push(Command::new("approve").about("Approve a trade to be executed"));
        self
    }

    pub fn entry_trade(mut self) -> Self {
        self.subcommands.push(
            Command::new("entry").about("Execute manually the entry to the market of a trade"),
        );
        self
    }

    pub fn stop_trade(mut self) -> Self {
        self.subcommands
            .push(Command::new("stop").about("Execute manually the safety of a trade"));
        self
    }

    pub fn target_trade(mut self) -> Self {
        self.subcommands
            .push(Command::new("target").about("Execute manually the target of a trade"));
        self
    }
}
