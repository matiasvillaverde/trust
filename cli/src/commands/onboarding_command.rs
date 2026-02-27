use clap::{Arg, Command};

pub struct OnboardingCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl OnboardingCommandBuilder {
    pub fn new() -> Self {
        Self {
            command: Command::new("onboarding")
                .about("Initial secure setup and status for Trust CLI")
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

    pub fn init(mut self) -> Self {
        self.subcommands.push(
            Command::new("init")
                .about("Initialize security controls for agent-safe usage")
                .arg(
                    Arg::new("protected-keyword")
                        .long("protected-keyword")
                        .value_name("KEYWORD")
                        .help("Protected keyword to store in keychain")
                        .required(true),
                ),
        );
        self
    }

    pub fn status(mut self) -> Self {
        self.subcommands.push(
            Command::new("status").about("Show onboarding and security-control readiness status"),
        );
        self
    }
}

impl Default for OnboardingCommandBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onboarding_builder_subcommands_present() {
        let cmd = OnboardingCommandBuilder::new().init().status().build();
        let names: Vec<&str> = cmd.get_subcommands().map(|s| s.get_name()).collect();
        assert!(names.contains(&"init"));
        assert!(names.contains(&"status"));
    }

    #[test]
    fn test_onboarding_init_and_status_parse() {
        let init_cmd = OnboardingCommandBuilder::new().init().build();
        let init = init_cmd
            .try_get_matches_from([
                "onboarding",
                "--format",
                "json",
                "init",
                "--protected-keyword",
                "top-secret",
            ])
            .expect("onboarding init should parse");
        assert_eq!(
            init.get_one::<String>("format").map(String::as_str),
            Some("json")
        );
        let init_sub = init.subcommand_matches("init").expect("init subcommand");
        assert_eq!(
            init_sub
                .get_one::<String>("protected-keyword")
                .map(String::as_str),
            Some("top-secret")
        );

        let status_cmd = OnboardingCommandBuilder::new().status().build();
        let status = status_cmd
            .try_get_matches_from(["onboarding", "status"])
            .expect("onboarding status should parse");
        assert!(status.subcommand_matches("status").is_some());
    }

    #[test]
    fn test_onboarding_default_matches_new() {
        let from_default = OnboardingCommandBuilder::default().status().build();
        let from_new = OnboardingCommandBuilder::new().status().build();
        assert_eq!(from_default.get_name(), from_new.get_name());
    }
}
