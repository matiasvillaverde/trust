use clap::{Arg, Command};

pub struct DbCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl DbCommandBuilder {
    pub fn new() -> Self {
        Self {
            command: Command::new("db")
                .about("Database backup/export and restore/import")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn export(mut self) -> Self {
        self.subcommands.push(
            Command::new("export")
                .about("Export full SQLite DB contents to a JSON file")
                .arg(
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .value_name("FILE")
                        .help("Output file path (default: trust-backup.json)")
                        .required(false),
                ),
        );
        self
    }

    pub fn import(mut self) -> Self {
        self.subcommands.push(
            Command::new("import")
                .about("Import a full JSON backup into SQLite")
                .arg(
                    Arg::new("input")
                        .short('i')
                        .long("input")
                        .value_name("FILE")
                        .help("Backup JSON file to import")
                        .required(true),
                )
                .arg(
                    Arg::new("mode")
                        .long("mode")
                        .value_name("MODE")
                        .help("Import mode (strict|replace) (default: strict)")
                        .value_parser(["strict", "replace"])
                        .required(false),
                )
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .help("Validate only; do not write anything")
                        .required(false)
                        .action(clap::ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("confirm-protected")
                        .long("confirm-protected")
                        .value_name("KEYWORD")
                        .help("Protected mutation keyword")
                        .required(false),
                ),
        );
        self
    }
}

impl Default for DbCommandBuilder {
    fn default() -> Self {
        Self::new()
    }
}
