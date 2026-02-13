use clap::{Arg, Command};

pub struct AccountCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl AccountCommandBuilder {
    pub fn new() -> Self {
        AccountCommandBuilder {
            command: Command::new("account")
                .about("Manage the trading account information")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn create_account(mut self) -> Self {
        self.subcommands.push(
            Command::new("create").about("Create a new account").arg(
                Arg::new("confirm-protected")
                    .long("confirm-protected")
                    .value_name("KEYWORD")
                    .help("Protected mutation keyword")
                    .required(false),
            ),
        );
        self
    }

    pub fn read_account(mut self) -> Self {
        self.subcommands
            .push(Command::new("search").about("search an account by name"));
        self
    }
}
