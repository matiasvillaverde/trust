use clap::Command;

pub struct AccountCommandBuilder {
    command: Vec<Command>,
}

impl AccountCommandBuilder {
    pub fn new() -> Self {
        AccountCommandBuilder { command: vec![] }
    }

    pub fn build(self) -> Vec<Command> {
        self.command
    }

    pub fn create_account(mut self) -> Self {
        let command = Command::new("account")
            .about("Manage the trading account information")
            .arg_required_else_help(true)
            .subcommand(Command::new("create").about("Create a new account"));
        self.command.push(command);
        self
    }
}
