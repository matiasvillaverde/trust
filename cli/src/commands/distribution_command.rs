use clap::{Arg, Command};

pub struct DistributionCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl DistributionCommandBuilder {
    pub fn new() -> Self {
        DistributionCommandBuilder {
            command: Command::new("distribution")
                .about("Manage profit distribution rules and execution")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn configure_distribution(mut self) -> Self {
        self.subcommands.push(
            Command::new("configure")
                .about("Configure distribution rules for an account")
                .arg(
                    Arg::new("account-id")
                        .long("account-id")
                        .short('a')
                        .value_name("UUID")
                        .help("Account ID to configure distribution rules for")
                        .required(true),
                )
                .arg(
                    Arg::new("earnings")
                        .long("earnings")
                        .short('e')
                        .value_name("PERCENT")
                        .help("Percentage for earnings allocation (e.g., 40.0 for 40%)")
                        .required(true),
                )
                .arg(
                    Arg::new("tax")
                        .long("tax")
                        .short('t')
                        .value_name("PERCENT")
                        .help("Percentage for tax reserve allocation (e.g., 30.0 for 30%)")
                        .required(true),
                )
                .arg(
                    Arg::new("reinvestment")
                        .long("reinvestment")
                        .short('r')
                        .value_name("PERCENT")
                        .help("Percentage for reinvestment allocation (e.g., 30.0 for 30%)")
                        .required(true),
                )
                .arg(
                    Arg::new("threshold")
                        .long("threshold")
                        .short('m')
                        .value_name("DECIMAL")
                        .help("Minimum profit threshold for distribution")
                        .required(true),
                )
                .arg(
                    Arg::new("password")
                        .long("password")
                        .short('p')
                        .value_name("STRING")
                        .help("Password required to create/update distribution rules")
                        .required(false),
                ),
        );
        self
    }

    pub fn execute_distribution(mut self) -> Self {
        self.subcommands.push(
            Command::new("execute")
                .about("Execute profit distribution for an account")
                .arg(
                    Arg::new("account-id")
                        .long("account-id")
                        .short('a')
                        .value_name("UUID")
                        .help("Source account ID for distribution")
                        .required(true),
                )
                .arg(
                    Arg::new("amount")
                        .long("amount")
                        .short('m')
                        .value_name("DECIMAL")
                        .help("Profit amount to distribute")
                        .required(true),
                ),
        );
        self
    }

    pub fn history(mut self) -> Self {
        self.subcommands.push(
            Command::new("history")
                .about("Show profit distribution execution history for an account")
                .arg(
                    Arg::new("account-id")
                        .long("account-id")
                        .short('a')
                        .value_name("UUID")
                        .help("Account ID to show distribution history for")
                        .required(true),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .short('l')
                        .value_name("N")
                        .help("Maximum number of entries to show (default: 20)")
                        .required(false),
                ),
        );
        self
    }

    pub fn show_rules(mut self) -> Self {
        self.subcommands.push(
            Command::new("rules")
                .about("Show distribution rules for an account")
                .subcommand(
                    Command::new("show")
                        .about("Show distribution rules for an account")
                        .arg(
                            Arg::new("account-id")
                                .long("account-id")
                                .value_name("UUID")
                                .help("Account ID to show distribution rules for")
                                .required(true),
                        ),
                )
                .arg(
                    Arg::new("account-id")
                        .long("account-id")
                        .value_name("UUID")
                        .help("Account ID to show distribution rules for (legacy: distribution rules --account-id ...)")
                        .required(false),
                ),
        );
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configure_allows_missing_password_flag() {
        let cmd = DistributionCommandBuilder::new()
            .configure_distribution()
            .execute_distribution()
            .history()
            .build();

        let account_id = "550e8400-e29b-41d4-a716-446655440000";
        let result = cmd.try_get_matches_from([
            "distribution",
            "configure",
            "--account-id",
            account_id,
            "--earnings",
            "40.0",
            "--tax",
            "30.0",
            "--reinvestment",
            "30.0",
            "--threshold",
            "100.0",
        ]);

        assert!(result.is_ok());
    }

    #[test]
    fn history_subcommand_parses() {
        let cmd = DistributionCommandBuilder::new().history().build();
        let account_id = "550e8400-e29b-41d4-a716-446655440000";
        let result =
            cmd.try_get_matches_from(["distribution", "history", "--account-id", account_id]);
        assert!(result.is_ok());
    }

    #[test]
    fn rules_subcommand_parses() {
        let cmd = DistributionCommandBuilder::new().show_rules().build();
        let account_id = "550e8400-e29b-41d4-a716-446655440000";
        let result =
            cmd.try_get_matches_from(["distribution", "rules", "--account-id", account_id]);
        assert!(result.is_ok());
    }

    #[test]
    fn rules_show_subcommand_parses() {
        let cmd = DistributionCommandBuilder::new().show_rules().build();
        let account_id = "550e8400-e29b-41d4-a716-446655440000";
        let result =
            cmd.try_get_matches_from(["distribution", "rules", "show", "--account-id", account_id]);
        assert!(result.is_ok());
    }
}
