use clap::{Arg, Command};

pub struct GradeCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl GradeCommandBuilder {
    pub fn new() -> Self {
        GradeCommandBuilder {
            command: Command::new("grade")
                .about("Grade closed trades and report grading statistics")
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

    pub fn show(mut self) -> Self {
        self.subcommands.push(
            Command::new("show")
                .about("Show a detailed grade report for a single trade")
                .arg(
                    Arg::new("trade_id")
                        .value_name("TRADE_ID")
                        .help("Trade UUID to grade/show")
                        .required(true),
                )
                .arg(
                    Arg::new("regrade")
                        .long("regrade")
                        .help("Recompute and persist a fresh grade even if one already exists")
                        .action(clap::ArgAction::SetTrue)
                        .required(false),
                )
                .arg(
                    Arg::new("weights")
                        .long("weights")
                        .value_name("WEIGHTS")
                        .help("Component weights as comma-separated percentages (e.g. 40,30,20,10) or permille (e.g. 400,300,200,100). Sum must be 100 or 1000.")
                        .required(false),
                ),
        );
        self
    }

    pub fn summary(mut self) -> Self {
        self.subcommands.push(
            Command::new("summary")
                .about("Summarize grades over a time window")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT_ID")
                        .help("Filter by specific account ID")
                        .required(false),
                )
                .arg(
                    Arg::new("days")
                        .long("days")
                        .value_name("DAYS")
                        .help("Window size in days (default: 30)")
                        .value_parser(clap::value_parser!(u32))
                        .required(false),
                )
                .arg(
                    Arg::new("weights")
                        .long("weights")
                        .value_name("WEIGHTS")
                        .help("Component weights as comma-separated percentages (e.g. 40,30,20,10) or permille (e.g. 400,300,200,100). Sum must be 100 or 1000.")
                        .required(false),
                ),
        );
        self
    }
}

impl Default for GradeCommandBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::GradeCommandBuilder;

    #[test]
    fn grade_builder_registers_subcommands() {
        let cmd = GradeCommandBuilder::new().show().summary().build();
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "show"));
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "summary"));
    }

    #[test]
    fn grade_show_parses_required_and_optional_args() {
        let cmd = GradeCommandBuilder::new().show().build();
        let matches = cmd
            .try_get_matches_from([
                "grade",
                "--format",
                "json",
                "show",
                "trade-id",
                "--regrade",
                "--weights",
                "40,30,20,10",
            ])
            .expect("grade show should parse");
        assert_eq!(
            matches.get_one::<String>("format").map(String::as_str),
            Some("json")
        );
        let show = matches.subcommand_matches("show").expect("show subcommand");
        assert_eq!(
            show.get_one::<String>("trade_id").map(String::as_str),
            Some("trade-id")
        );
        assert!(show.get_flag("regrade"));
        assert_eq!(
            show.get_one::<String>("weights").map(String::as_str),
            Some("40,30,20,10")
        );
    }

    #[test]
    fn grade_summary_parses_window_and_account() {
        let cmd = GradeCommandBuilder::new().summary().build();
        let matches = cmd
            .try_get_matches_from([
                "grade",
                "summary",
                "--account",
                "acc-1",
                "--days",
                "30",
                "--weights",
                "400,300,200,100",
            ])
            .expect("grade summary should parse");
        let summary = matches
            .subcommand_matches("summary")
            .expect("summary subcommand");
        assert_eq!(
            summary.get_one::<String>("account").map(String::as_str),
            Some("acc-1")
        );
        assert_eq!(summary.get_one::<u32>("days"), Some(&30));
        assert_eq!(
            summary.get_one::<String>("weights").map(String::as_str),
            Some("400,300,200,100")
        );
    }

    #[test]
    fn default_matches_new() {
        let from_default = GradeCommandBuilder::default().show().build();
        let from_new = GradeCommandBuilder::new().show().build();
        assert_eq!(from_default.get_name(), from_new.get_name());
    }
}
