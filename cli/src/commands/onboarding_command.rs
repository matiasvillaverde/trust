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
}
