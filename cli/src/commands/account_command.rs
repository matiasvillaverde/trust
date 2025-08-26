use clap::{Arg, Command};

pub struct AccountCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl AccountCommandBuilder {
    pub fn new() -> Self {
        AccountCommandBuilder {
            command: Command::new("account")
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
                .about("Create a new account with optional hierarchy support")
                .arg(
                    Arg::new("name")
                        .long("name")
                        .short('n')
                        .value_name("NAME")
                        .help("Account name")
                        .required(true),
                )
                .arg(
                    Arg::new("type")
                        .long("type")
                        .short('t')
                        .value_name("TYPE")
                        .help("Account type (Primary, Earnings, TaxReserve, Reinvestment)")
                        .value_parser(["Primary", "Earnings", "TaxReserve", "Reinvestment"])
                        .default_value("Primary"),
                )
                .arg(
                    Arg::new("parent-id")
                        .long("parent-id")
                        .short('p')
                        .value_name("UUID")
                        .help("Parent account ID for hierarchical accounts")
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
}
