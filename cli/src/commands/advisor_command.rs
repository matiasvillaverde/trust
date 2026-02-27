use clap::{Arg, Command};

pub struct AdvisorCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl AdvisorCommandBuilder {
    pub fn new() -> Self {
        Self {
            command: Command::new("advisor")
                .about("Configure and run advisory concentration checks")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn configure(mut self) -> Self {
        self.subcommands.push(
            Command::new("configure")
                .about("Configure advisory thresholds for an account")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT_ID")
                        .required(true),
                )
                .arg(
                    Arg::new("sector-limit")
                        .long("sector-limit")
                        .value_name("PERCENT")
                        .required(true),
                )
                .arg(
                    Arg::new("asset-class-limit")
                        .long("asset-class-limit")
                        .value_name("PERCENT")
                        .required(true),
                )
                .arg(
                    Arg::new("single-position-limit")
                        .long("single-position-limit")
                        .value_name("PERCENT")
                        .required(true),
                )
                .arg(
                    Arg::new("confirm-protected")
                        .long("confirm-protected")
                        .value_name("KEYWORD")
                        .required(false),
                ),
        );
        self
    }

    pub fn check(mut self) -> Self {
        self.subcommands.push(
            Command::new("check")
                .about("Run advisory check for a proposed trade")
                .arg(Arg::new("account").long("account").required(true))
                .arg(Arg::new("symbol").long("symbol").required(true))
                .arg(Arg::new("entry").long("entry").required(true))
                .arg(Arg::new("quantity").long("quantity").required(true))
                .arg(Arg::new("sector").long("sector").required(false))
                .arg(Arg::new("asset-class").long("asset-class").required(false)),
        );
        self
    }

    pub fn status(mut self) -> Self {
        self.subcommands.push(
            Command::new("status")
                .about("Show advisory portfolio status for an account")
                .arg(Arg::new("account").long("account").required(true)),
        );
        self
    }

    pub fn history(mut self) -> Self {
        self.subcommands.push(
            Command::new("history")
                .about("Show advisory history for an account")
                .arg(Arg::new("account").long("account").required(true))
                .arg(Arg::new("days").long("days").required(false)),
        );
        self
    }
}

impl Default for AdvisorCommandBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configure_parses() {
        let cmd = AdvisorCommandBuilder::new().configure().build();
        let result = cmd.try_get_matches_from([
            "advisor",
            "configure",
            "--account",
            "550e8400-e29b-41d4-a716-446655440000",
            "--sector-limit",
            "30",
            "--asset-class-limit",
            "40",
            "--single-position-limit",
            "15",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn check_parses() {
        let cmd = AdvisorCommandBuilder::new().check().build();
        let result = cmd.try_get_matches_from([
            "advisor",
            "check",
            "--account",
            "550e8400-e29b-41d4-a716-446655440000",
            "--symbol",
            "AAPL",
            "--entry",
            "150",
            "--quantity",
            "100",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn status_and_history_parse() {
        let status_cmd = AdvisorCommandBuilder::new().status().build();
        let status = status_cmd
            .try_get_matches_from([
                "advisor",
                "status",
                "--account",
                "550e8400-e29b-41d4-a716-446655440000",
            ])
            .expect("status should parse");
        let status_sub = status
            .subcommand_matches("status")
            .expect("status subcommand");
        assert_eq!(
            status_sub.get_one::<String>("account").map(String::as_str),
            Some("550e8400-e29b-41d4-a716-446655440000")
        );

        let history_cmd = AdvisorCommandBuilder::new().history().build();
        let history = history_cmd
            .try_get_matches_from([
                "advisor",
                "history",
                "--account",
                "550e8400-e29b-41d4-a716-446655440000",
                "--days",
                "7",
            ])
            .expect("history should parse");
        let history_sub = history
            .subcommand_matches("history")
            .expect("history subcommand");
        assert_eq!(
            history_sub.get_one::<String>("account").map(String::as_str),
            Some("550e8400-e29b-41d4-a716-446655440000")
        );
        assert_eq!(
            history_sub.get_one::<String>("days").map(String::as_str),
            Some("7")
        );
    }

    #[test]
    fn default_matches_new() {
        let from_default = AdvisorCommandBuilder::default().configure().build();
        let from_new = AdvisorCommandBuilder::new().configure().build();
        assert_eq!(from_default.get_name(), from_new.get_name());
    }
}
