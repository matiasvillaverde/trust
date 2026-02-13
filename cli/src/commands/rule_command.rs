use clap::{Arg, Command};

pub struct RuleCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl RuleCommandBuilder {
    pub fn new() -> Self {
        RuleCommandBuilder {
            command: Command::new("rule")
                .about("Manage rules for your account")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn create_rule(mut self) -> Self {
        self.subcommands.push(
            Command::new("create")
                .about("Create a new rule to your account")
                .arg(
                    Arg::new("confirm-protected")
                        .long("confirm-protected")
                        .value_name("KEYWORD")
                        .help("Required confirmation keyword for protected risk mutations")
                        .required(false),
                ),
        );
        self
    }

    pub fn remove_rule(mut self) -> Self {
        self.subcommands.push(
            Command::new("remove")
                .about("Remove a new rule from your account")
                .arg(
                    Arg::new("confirm-protected")
                        .long("confirm-protected")
                        .value_name("KEYWORD")
                        .help("Required confirmation keyword for protected risk mutations")
                        .required(false),
                ),
        );
        self
    }
}
