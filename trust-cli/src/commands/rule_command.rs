use clap::Command;

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
        self.subcommands
            .push(Command::new("add").about("Add a new rule to your account"));
        self
    }

    pub fn remove_rule(mut self) -> Self {
        self.subcommands
            .push(Command::new("remove").about("Remove a new rule from your account"));
        self
    }
}
