use clap::{Arg, Command};

pub struct PolicyCommandBuilder {
    command: Command,
}

impl PolicyCommandBuilder {
    pub fn new() -> Self {
        Self {
            command: Command::new("policy")
                .about("Show CLI operational policy and protection boundaries")
                .arg(
                    Arg::new("format")
                        .long("format")
                        .value_name("FORMAT")
                        .help("Output format")
                        .value_parser(["text", "json"])
                        .default_value("text"),
                ),
        }
    }

    pub fn build(self) -> Command {
        self.command
    }
}

impl Default for PolicyCommandBuilder {
    fn default() -> Self {
        Self::new()
    }
}
