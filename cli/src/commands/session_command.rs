use clap::{Arg, Command};

pub struct SessionCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl SessionCommandBuilder {
    pub fn new() -> Self {
        Self {
            command: Command::new("session")
                .about("Open, close, and review trading session plans")
                .arg(
                    Arg::new("format")
                        .long("format")
                        .value_name("FORMAT")
                        .help("Output format")
                        .value_parser(["human", "text", "json"])
                        .default_value("human")
                        .global(true),
                )
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn open(mut self) -> Self {
        self.subcommands.push(
            Command::new("open")
                .about("Open a trading session and capture the pre-session plan")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Account name or UUID")
                        .required(true),
                ),
        );
        self
    }

    pub fn close(mut self) -> Self {
        self.subcommands.push(
            Command::new("close")
                .about("Close the active trading session and capture the review")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Account name or UUID; required when more than one session is open")
                        .required(false),
                ),
        );
        self
    }

    pub fn list(mut self) -> Self {
        self.subcommands.push(
            Command::new("list")
                .about("List session plan history for an account")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Account name or UUID")
                        .required(true),
                ),
        );
        self
    }
}

impl Default for SessionCommandBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_registers_session_subcommands() {
        let cmd = SessionCommandBuilder::new().open().close().list().build();
        let names: Vec<&str> = cmd.get_subcommands().map(|sub| sub.get_name()).collect();
        assert!(names.contains(&"open"));
        assert!(names.contains(&"close"));
        assert!(names.contains(&"list"));
    }

    #[test]
    fn commands_parse_required_fields_and_formats() {
        let cmd = SessionCommandBuilder::new().open().close().list().build();

        let open = cmd
            .clone()
            .try_get_matches_from(["session", "open", "--account", "trading"])
            .expect("session open should parse");
        assert!(open.subcommand_matches("open").is_some());

        let close = cmd
            .clone()
            .try_get_matches_from(["session", "close"])
            .expect("session close should parse without account");
        assert!(close.subcommand_matches("close").is_some());

        let list = cmd
            .try_get_matches_from([
                "session",
                "--format",
                "json",
                "list",
                "--account",
                "trading",
            ])
            .expect("session list should parse");
        assert_eq!(
            list.get_one::<String>("format").map(String::as_str),
            Some("json")
        );
        assert!(list.subcommand_matches("list").is_some());
    }
}
