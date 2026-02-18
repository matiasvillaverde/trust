use clap::{Arg, Command};

pub struct AccountCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl AccountCommandBuilder {
    pub fn new() -> Self {
        AccountCommandBuilder {
            command: Command::new("accounts")
                .visible_alias("account")
                .about("Manage the trading account information")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn create_account(mut self) -> Self {
        self.subcommands.push(
            Command::new("create")
                .about("Create a new account")
                .arg(
                    Arg::new("name")
                        .long("name")
                        .value_name("STRING")
                        .help("Account name (non-interactive mode)")
                        .required(false),
                )
                .arg(
                    Arg::new("description")
                        .long("description")
                        .value_name("STRING")
                        .help("Account description (non-interactive mode)")
                        .required(false),
                )
                .arg(
                    Arg::new("environment")
                        .long("environment")
                        .value_name("ENV")
                        .help("Environment: paper|live (non-interactive mode)")
                        .required(false),
                )
                .arg(
                    Arg::new("taxes")
                        .long("taxes")
                        .value_name("DECIMAL")
                        .help("Tax percentage (e.g. 25.0) (non-interactive mode)")
                        .required(false),
                )
                .arg(
                    Arg::new("earnings")
                        .long("earnings")
                        .value_name("DECIMAL")
                        .help("Earnings percentage (e.g. 30.0) (non-interactive mode)")
                        .required(false),
                )
                .arg(
                    Arg::new("type")
                        .long("type")
                        .value_name("TYPE")
                        .help("Account type: primary|earnings|tax-reserve|reinvestment")
                        .required(false),
                )
                .arg(
                    Arg::new("parent")
                        .long("parent")
                        .value_name("UUID")
                        .help("Parent account ID for child account types")
                        .required(false),
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

    pub fn read_account(mut self) -> Self {
        self.subcommands
            .push(Command::new("search").about("search an account by name"));
        self
    }

    pub fn transfer_account(mut self) -> Self {
        self.subcommands.push(
            Command::new("transfer")
                .about("Transfer funds between accounts in hierarchy")
                .arg(
                    Arg::new("from")
                        .long("from")
                        .short('f')
                        .value_name("UUID")
                        .help("Source account ID")
                        .required(true),
                )
                .arg(
                    Arg::new("to")
                        .long("to")
                        .short('t')
                        .value_name("UUID")
                        .help("Destination account ID")
                        .required(true),
                )
                .arg(
                    Arg::new("amount")
                        .long("amount")
                        .short('a')
                        .value_name("DECIMAL")
                        .help("Amount to transfer")
                        .required(true),
                )
                .arg(
                    Arg::new("reason")
                        .long("reason")
                        .short('r')
                        .value_name("STRING")
                        .help("Reason for transfer")
                        .required(true),
                ),
        );
        self
    }

    pub fn list_accounts(mut self) -> Self {
        self.subcommands.push(
            Command::new("list").about("List accounts").arg(
                Arg::new("hierarchy")
                    .long("hierarchy")
                    .help("Display account hierarchy")
                    .action(clap::ArgAction::SetTrue)
                    .required(false),
            ),
        );
        self
    }

    pub fn balance_accounts(mut self) -> Self {
        self.subcommands.push(
            Command::new("balance").about("Show account balances").arg(
                Arg::new("detailed")
                    .long("detailed")
                    .help("Show detailed balances per account and currency")
                    .action(clap::ArgAction::SetTrue)
                    .required(false),
            ),
        );
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_non_interactive_parses() {
        let cmd = AccountCommandBuilder::new().create_account().build();
        let parent = "550e8400-e29b-41d4-a716-446655440000";
        let result = cmd.try_get_matches_from([
            "accounts",
            "create",
            "--name",
            "Tax Reserve",
            "--description",
            "Tax",
            "--environment",
            "paper",
            "--taxes",
            "25",
            "--earnings",
            "30",
            "--type",
            "tax-reserve",
            "--parent",
            parent,
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn list_hierarchy_parses() {
        let cmd = AccountCommandBuilder::new().list_accounts().build();
        let result = cmd.try_get_matches_from(["accounts", "list", "--hierarchy"]);
        assert!(result.is_ok());
    }

    #[test]
    fn balance_detailed_parses() {
        let cmd = AccountCommandBuilder::new().balance_accounts().build();
        let result = cmd.try_get_matches_from(["accounts", "balance", "--detailed"]);
        assert!(result.is_ok());
    }
}
