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
}
