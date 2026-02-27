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

    pub fn progress(mut self) -> Self {
        self.subcommands.push(
            Command::new("progress")
                .about("Show what is missing to move one level up/down")
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
                ),
        );
        self
    }

    pub fn rules(mut self) -> Self {
        self.subcommands.push(
            Command::new("rules")
                .about("Manage level-adjustment policy rules")
                .subcommand(
                    Command::new("show")
                        .about("Show policy rules for an account")
                        .arg(
                            Arg::new("account")
                                .long("account")
                                .short('a')
                                .value_name("ACCOUNT_ID")
                                .help("Target account ID")
                                .required(true),
                        ),
                )
                .subcommand(
                    Command::new("set")
                        .about("Set one policy rule value for an account")
                        .arg(
                            Arg::new("account")
                                .long("account")
                                .short('a')
                                .value_name("ACCOUNT_ID")
                                .help("Target account ID")
                                .required(true),
                        )
                        .arg(
                            Arg::new("rule")
                                .long("rule")
                                .value_name("RULE_KEY")
                                .help("Rule key to update")
                                .value_parser([
                                    "monthly_loss_downgrade_pct",
                                    "single_loss_downgrade_pct",
                                    "upgrade_profitable_trades",
                                    "upgrade_win_rate_pct",
                                    "upgrade_consecutive_wins",
                                    "cooldown_profitable_trades",
                                    "cooldown_win_rate_pct",
                                    "cooldown_consecutive_wins",
                                    "recovery_profitable_trades",
                                    "recovery_win_rate_pct",
                                    "recovery_consecutive_wins",
                                    "min_trades_at_level_for_upgrade",
                                    "max_changes_in_30_days",
                                ])
                                .required(true),
                        )
                        .arg(
                            Arg::new("value")
                                .long("value")
                                .value_name("VALUE")
                                .help("New value for selected rule")
                                .required(true),
                        )
                        .arg(
                            Arg::new("confirm-protected")
                                .long("confirm-protected")
                                .value_name("KEYWORD")
                                .help("Required confirmation keyword for protected risk mutations")
                                .required(false),
                        ),
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
            .progress()
            .rules()
            .build();

        let names: Vec<&str> = cmd.get_subcommands().map(|s| s.get_name()).collect();
        assert!(names.contains(&"status"));
        assert!(names.contains(&"triggers"));
        assert!(names.contains(&"history"));
        assert!(names.contains(&"change"));
        assert!(names.contains(&"evaluate"));
        assert!(names.contains(&"progress"));
        assert!(names.contains(&"rules"));
    }

    #[test]
    fn test_level_subcommand_parsing_shapes() {
        let status_cmd = LevelCommandBuilder::new().status().build();
        let status = status_cmd
            .try_get_matches_from([
                "level",
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

        let history_cmd = LevelCommandBuilder::new().history().build();
        let history = history_cmd
            .try_get_matches_from(["level", "history", "--account", "acc-1", "--days", "30"])
            .expect("history should parse");
        let history_sub = history
            .subcommand_matches("history")
            .expect("history subcommand");
        assert_eq!(
            history_sub.get_one::<String>("account").map(String::as_str),
            Some("acc-1")
        );
        assert_eq!(history_sub.get_one::<u32>("days"), Some(&30));

        let rules_cmd = LevelCommandBuilder::new().rules().build();
        let show = rules_cmd
            .clone()
            .try_get_matches_from(["level", "rules", "show", "--account", "acc-1"])
            .expect("rules show should parse");
        let rules_show = show
            .subcommand_matches("rules")
            .expect("rules subcommand")
            .subcommand_matches("show")
            .expect("rules show subcommand");
        assert_eq!(
            rules_show.get_one::<String>("account").map(String::as_str),
            Some("acc-1")
        );

        let set = rules_cmd
            .try_get_matches_from([
                "level",
                "rules",
                "set",
                "--account",
                "acc-1",
                "--rule",
                "upgrade_profitable_trades",
                "--value",
                "7",
            ])
            .expect("rules set should parse");
        let rules_set = set
            .subcommand_matches("rules")
            .expect("rules subcommand")
            .subcommand_matches("set")
            .expect("rules set subcommand");
        assert_eq!(
            rules_set.get_one::<String>("account").map(String::as_str),
            Some("acc-1")
        );
        assert_eq!(
            rules_set.get_one::<String>("rule").map(String::as_str),
            Some("upgrade_profitable_trades")
        );
        assert_eq!(
            rules_set.get_one::<String>("value").map(String::as_str),
            Some("7")
        );
    }

    #[test]
    fn test_level_default_matches_new() {
        let from_default = LevelCommandBuilder::default().status().build();
        let from_new = LevelCommandBuilder::new().status().build();
        assert_eq!(from_default.get_name(), from_new.get_name());
    }
}
