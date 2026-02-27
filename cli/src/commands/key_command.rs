use clap::{Arg, Command};

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
        self.subcommands.push(
            Command::new("create")
                .about("Create new keys for trading environment")
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

    pub fn read_environment(mut self) -> Self {
        self.subcommands
            .push(Command::new("show").about("Show the current environment and url"));
        self
    }

    pub fn delete_environment(mut self) -> Self {
        self.subcommands.push(
            Command::new("delete")
                .about("Delete the current environment and url")
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

    pub fn protected_set(mut self) -> Self {
        self.subcommands.push(
            Command::new("protected-set")
                .about("Store the protected mutation keyword in keychain")
                .arg(
                    Arg::new("value")
                        .long("value")
                        .value_name("KEYWORD")
                        .help("Protected keyword to store in keychain")
                        .required(true),
                )
                .arg(
                    Arg::new("confirm-protected")
                        .long("confirm-protected")
                        .value_name("CURRENT_KEYWORD")
                        .help("Current keyword (required when rotating an existing keyword)")
                        .required(false),
                ),
        );
        self
    }

    pub fn protected_show(mut self) -> Self {
        self.subcommands.push(
            Command::new("protected-show")
                .about("Show whether protected mutation keyword is configured"),
        );
        self
    }

    pub fn protected_delete(mut self) -> Self {
        self.subcommands.push(
            Command::new("protected-delete")
                .about("Delete protected mutation keyword")
                .arg(
                    Arg::new("confirm-protected")
                        .long("confirm-protected")
                        .value_name("CURRENT_KEYWORD")
                        .help("Current keyword")
                        .required(true),
                ),
        );
        self
    }
}

#[cfg(test)]
mod tests {
    use super::KeysCommandBuilder;

    #[test]
    fn keys_builder_registers_all_subcommands() {
        let cmd = KeysCommandBuilder::new()
            .create_keys()
            .read_environment()
            .delete_environment()
            .protected_set()
            .protected_show()
            .protected_delete()
            .build();
        for name in [
            "create",
            "show",
            "delete",
            "protected-set",
            "protected-show",
            "protected-delete",
        ] {
            assert!(cmd.get_subcommands().any(|c| c.get_name() == name));
        }
    }

    #[test]
    fn keys_protected_set_parses_required_value() {
        let cmd = KeysCommandBuilder::new().protected_set().build();
        let matches = cmd
            .try_get_matches_from([
                "keys",
                "protected-set",
                "--value",
                "new-keyword",
                "--confirm-protected",
                "old-keyword",
            ])
            .expect("protected-set should parse");
        let sub = matches
            .subcommand_matches("protected-set")
            .expect("protected-set subcommand");
        assert_eq!(
            sub.get_one::<String>("value").map(String::as_str),
            Some("new-keyword")
        );
        assert_eq!(
            sub.get_one::<String>("confirm-protected")
                .map(String::as_str),
            Some("old-keyword")
        );
    }
}
