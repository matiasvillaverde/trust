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

#[cfg(test)]
mod tests {
    use super::PolicyCommandBuilder;

    #[test]
    fn policy_defaults_to_text_format() {
        let cmd = PolicyCommandBuilder::new().build();
        let matches = cmd
            .try_get_matches_from(["policy"])
            .expect("policy should parse");
        assert_eq!(
            matches.get_one::<String>("format").map(String::as_str),
            Some("text")
        );
    }

    #[test]
    fn policy_accepts_json_format() {
        let cmd = PolicyCommandBuilder::new().build();
        let matches = cmd
            .try_get_matches_from(["policy", "--format", "json"])
            .expect("policy json should parse");
        assert_eq!(
            matches.get_one::<String>("format").map(String::as_str),
            Some("json")
        );
    }

    #[test]
    fn policy_default_matches_new() {
        let from_default = PolicyCommandBuilder::default().build();
        let from_new = PolicyCommandBuilder::new().build();
        assert_eq!(from_default.get_name(), from_new.get_name());
    }
}
