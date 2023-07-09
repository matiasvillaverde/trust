use clap::Command;

pub struct KeysCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl KeysCommandBuilder {
    pub fn new() -> Self {
        KeysCommandBuilder {
            command: Command::new("keys")
                .about("Manage the keys for the trading environment")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn create_keys(mut self) -> Self {
        self.subcommands
            .push(Command::new("create").about("Create new keys for trading environment"));
        self
    }

    pub fn read_environment(mut self) -> Self {
        self.subcommands
            .push(Command::new("show").about("Show the current environment and url"));
        self
    }

    pub fn delete_environment(mut self) -> Self {
        self.subcommands
            .push(Command::new("delete").about("Delete the current environment and url"));
        self
    }
}
