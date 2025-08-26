use clap::{Arg, Command};

pub struct LevelCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl LevelCommandBuilder {
    pub fn new() -> Self {
        LevelCommandBuilder {
            command: Command::new("level")
                .about("Manage trading levels and view level status")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn status_command(mut self) -> Self {
        self.subcommands.push(
            Command::new("status")
                .about("Show current level information")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .short('a')
                        .value_name("ACCOUNT_ID")
                        .help("Account ID to check level for")
                        .required(false),
                ),
        );
        self
    }

    pub fn history_command(mut self) -> Self {
        self.subcommands.push(
            Command::new("history")
                .about("Show detailed level change log")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .short('a')
                        .value_name("ACCOUNT_ID")
                        .help("Account ID to show history for")
                        .required(false),
                )
                .arg(
                    Arg::new("days")
                        .long("days")
                        .short('d')
                        .value_name("DAYS")
                        .help("Number of days to show (default: 90)")
                        .value_parser(clap::value_parser!(u32))
                        .default_value("90"),
                ),
        );
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_command_builder_new() {
        let builder = LevelCommandBuilder::new();
        let command = builder.build();

        assert_eq!(command.get_name(), "level");
        assert!(command.is_arg_required_else_help_set());
    }

    #[test]
    fn test_status_subcommand() {
        let builder = LevelCommandBuilder::new();
        let command = builder.status_command().build();

        let subcommands: Vec<&str> = command.get_subcommands().map(|c| c.get_name()).collect();
        assert!(subcommands.contains(&"status"));
    }

    #[test]
    fn test_history_subcommand() {
        let builder = LevelCommandBuilder::new();
        let command = builder.history_command().build();

        let subcommands: Vec<&str> = command.get_subcommands().map(|c| c.get_name()).collect();
        assert!(subcommands.contains(&"history"));
    }

    #[test]
    fn test_both_subcommands() {
        let builder = LevelCommandBuilder::new();
        let command = builder.status_command().history_command().build();

        let subcommands: Vec<&str> = command.get_subcommands().map(|c| c.get_name()).collect();
        assert!(subcommands.contains(&"status"));
        assert!(subcommands.contains(&"history"));
        assert_eq!(subcommands.len(), 2);
    }

    #[test]
    fn test_history_command_has_days_arg() {
        let builder = LevelCommandBuilder::new();
        let command = builder.history_command().build();

        let history_cmd = command
            .get_subcommands()
            .find(|c| c.get_name() == "history")
            .expect("History command should exist");

        let days_arg = history_cmd
            .get_arguments()
            .find(|a| a.get_id() == "days")
            .expect("Days argument should exist");

        assert_eq!(days_arg.get_default_values(), &["90"]);
    }
}
