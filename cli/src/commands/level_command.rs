use clap::{Arg, Command};

/// CLI builder for level management commands.
pub struct LevelCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl LevelCommandBuilder {
    pub fn new() -> Self {
        Self {
            command: Command::new("level")
                .about("Manage account risk levels")
                .arg(
                    Arg::new("format")
                        .long("format")
                        .value_name("FORMAT")
                        .help("Output format")
                        .value_parser(["text", "json"])
                        .default_value("text")
                        .global(true),
                )
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn status(mut self) -> Self {
        self.subcommands.push(
            Command::new("status")
                .about("Show current level information")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .short('a')
                        .value_name("ACCOUNT_ID")
                        .help("Account ID to inspect")
                        .required(false),
                ),
        );
        self
    }

    pub fn triggers(mut self) -> Self {
        self.subcommands
            .push(Command::new("triggers").about("List supported level trigger identifiers"));
        self
    }

    pub fn history(mut self) -> Self {
        self.subcommands.push(
            Command::new("history")
                .about("Show account level change history")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .short('a')
                        .value_name("ACCOUNT_ID")
                        .help("Account ID to inspect")
                        .required(false),
                )
                .arg(
                    Arg::new("days")
                        .long("days")
                        .short('d')
                        .value_name("DAYS")
                        .help("Optional days window filter")
                        .value_parser(clap::value_parser!(u32))
                        .required(false),
                ),
        );
        self
    }

    pub fn change(mut self) -> Self {
        self.subcommands.push(
            Command::new("change")
                .about("Manually change account level")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .short('a')
                        .value_name("ACCOUNT_ID")
                        .help("Target account ID")
                        .required(true),
                )
                .arg(
                    Arg::new("to")
                        .long("to")
                        .value_name("LEVEL")
                        .help("Target level (0-4)")
                        .value_parser(clap::value_parser!(u8).range(0..=4))
                        .required(true),
                )
                .arg(
                    Arg::new("reason")
                        .long("reason")
                        .value_name("TEXT")
                        .help("Human-readable reason for the change")
                        .required(true),
                )
                .arg(
                    Arg::new("trigger")
                        .long("trigger")
                        .value_name("TRIGGER")
                        .help("Trigger type (manual_override, manual_review, risk_breach, performance_upgrade, or custom)")
                        .default_value("manual_override")
                        .required(false),
                )
                .arg(
                    Arg::new("confirm-protected")
                        .long("confirm-protected")
                        .value_name("KEYWORD")
                        .help("Required confirmation keyword for protected level mutations")
                        .required(false),
                ),
        );
        self
    }

    pub fn evaluate(mut self) -> Self {
        self.subcommands.push(
            Command::new("evaluate")
                .about("Evaluate (and optionally apply) policy-based level transition")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .short('a')
                        .value_name("ACCOUNT_ID")
                        .help("Target account ID")
                        .required(true),
                )
                .arg(
                    Arg::new("profitable-trades")
                        .long("profitable-trades")
                        .value_name("COUNT")
                        .help("Profitable trades in evaluation window")
                        .value_parser(clap::value_parser!(u32))
                        .required(true),
                )
                .arg(
                    Arg::new("win-rate")
                        .long("win-rate")
                        .value_name("PERCENT")
                        .help("Win rate percentage, e.g. 70")
                        .required(true),
                )
                .arg(
                    Arg::new("monthly-loss")
                        .long("monthly-loss")
                        .value_name("PERCENT")
                        .help("Monthly loss percentage, use negative values for losses")
                        .required(true),
                )
                .arg(
                    Arg::new("largest-loss")
                        .long("largest-loss")
                        .value_name("PERCENT")
                        .help("Largest single-trade loss percentage, negative for losses")
                        .required(true),
                )
                .arg(
                    Arg::new("consecutive-wins")
                        .long("consecutive-wins")
                        .value_name("COUNT")
                        .help("Current consecutive wins")
                        .value_parser(clap::value_parser!(u32))
                        .default_value("0")
                        .required(false),
                )
                .arg(
                    Arg::new("apply")
                        .long("apply")
                        .help("Apply decision if policy recommends a transition")
                        .action(clap::ArgAction::SetTrue)
                        .required(false),
                )
                .arg(
                    Arg::new("confirm-protected")
                        .long("confirm-protected")
                        .value_name("KEYWORD")
                        .help("Required when --apply mutates levels")
                        .required(false),
                ),
        );
        self
    }
}

impl Default for LevelCommandBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_builder_subcommands_present() {
        let cmd = LevelCommandBuilder::new()
            .status()
            .triggers()
            .history()
            .change()
            .evaluate()
            .build();

        let names: Vec<&str> = cmd.get_subcommands().map(|s| s.get_name()).collect();
        assert!(names.contains(&"status"));
        assert!(names.contains(&"triggers"));
        assert!(names.contains(&"history"));
        assert!(names.contains(&"change"));
        assert!(names.contains(&"evaluate"));
    }
}
