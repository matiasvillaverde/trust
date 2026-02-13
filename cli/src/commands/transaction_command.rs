use clap::{Arg, Command};

pub struct TransactionCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl TransactionCommandBuilder {
    pub fn new() -> Self {
        TransactionCommandBuilder {
            command: Command::new("transaction")
                .about("Withdraw or deposit money from an account")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn deposit(mut self) -> Self {
        self.subcommands.push(
            Command::new("deposit")
                .about("Add money to an account")
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

    pub fn withdraw(mut self) -> Self {
        self.subcommands.push(
            Command::new("withdraw")
                .about("Withdraw money from an account")
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
}
