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

#[cfg(test)]
mod tests {
    use super::DbCommandBuilder;

    #[test]
    fn db_builder_registers_export_and_import() {
        let cmd = DbCommandBuilder::new().export().import().build();
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "export"));
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "import"));
    }

    #[test]
    fn db_export_parses_optional_output() {
        let cmd = DbCommandBuilder::new().export().build();
        let matches = cmd
            .try_get_matches_from(["db", "export", "--output", "backup.json"])
            .expect("db export should parse");
        let export = matches
            .subcommand_matches("export")
            .expect("export subcommand");
        assert_eq!(
            export.get_one::<String>("output").map(String::as_str),
            Some("backup.json")
        );
    }

    #[test]
    fn db_import_parses_required_and_optional_args() {
        let cmd = DbCommandBuilder::new().import().build();
        let matches = cmd
            .try_get_matches_from([
                "db",
                "import",
                "--input",
                "backup.json",
                "--mode",
                "replace",
                "--dry-run",
                "--confirm-protected",
                "keyword",
            ])
            .expect("db import should parse");
        let import = matches
            .subcommand_matches("import")
            .expect("import subcommand");
        assert_eq!(
            import.get_one::<String>("input").map(String::as_str),
            Some("backup.json")
        );
        assert_eq!(
            import.get_one::<String>("mode").map(String::as_str),
            Some("replace")
        );
        assert!(import.get_flag("dry-run"));
        assert_eq!(
            import
                .get_one::<String>("confirm-protected")
                .map(String::as_str),
            Some("keyword")
        );
    }

    #[test]
    fn default_matches_new() {
        let from_default = DbCommandBuilder::default().export().build();
        let from_new = DbCommandBuilder::new().export().build();
        assert_eq!(from_default.get_name(), from_new.get_name());
    }
}
