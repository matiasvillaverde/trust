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

