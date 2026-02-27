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

#[cfg(test)]
mod tests {
    use super::RuleCommandBuilder;

    #[test]
    fn rule_builder_registers_create_and_remove() {
        let cmd = RuleCommandBuilder::new()
            .create_rule()
            .remove_rule()
            .build();
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "create"));
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "remove"));
    }

    #[test]
    fn rule_create_parses_confirm_keyword() {
        let cmd = RuleCommandBuilder::new().create_rule().build();
        let matches = cmd
            .try_get_matches_from(["rule", "create", "--confirm-protected", "keyword"])
            .expect("rule create should parse");
        let sub = matches
            .subcommand_matches("create")
            .expect("create subcommand");
        assert_eq!(
            sub.get_one::<String>("confirm-protected")
                .map(String::as_str),
            Some("keyword")
        );
    }

    #[test]
    fn rule_remove_parses_confirm_keyword() {
        let cmd = RuleCommandBuilder::new().remove_rule().build();
        let matches = cmd
            .try_get_matches_from(["rule", "remove", "--confirm-protected", "keyword"])
            .expect("rule remove should parse");
        let sub = matches
            .subcommand_matches("remove")
            .expect("remove subcommand");
        assert_eq!(
            sub.get_one::<String>("confirm-protected")
                .map(String::as_str),
            Some("keyword")
        );
    }
}
