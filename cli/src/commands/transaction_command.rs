use clap::{Arg, Command};

pub struct TransactionCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl TransactionCommandBuilder {
    pub fn new() -> Self {
        TransactionCommandBuilder {
            command: Command::new("transaction")
                .about("Withdraw or deposit money from an account")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn deposit(mut self) -> Self {
        self.subcommands.push(
            Command::new("deposit")
                .about("Add money to an account")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Account name or UUID for non-interactive mode")
                        .required(false),
                )
                .arg(
                    Arg::new("currency")
                        .long("currency")
                        .value_name("CURRENCY")
                        .help("Currency code (USD, EUR, BTC) for non-interactive mode")
                        .required(false),
                )
                .arg(
                    Arg::new("amount")
                        .long("amount")
                        .value_name("AMOUNT")
                        .help("Decimal amount for non-interactive mode")
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

    pub fn withdraw(mut self) -> Self {
        self.subcommands.push(
            Command::new("withdraw")
                .about("Withdraw money from an account")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Account name or UUID for non-interactive mode")
                        .required(false),
                )
                .arg(
                    Arg::new("currency")
                        .long("currency")
                        .value_name("CURRENCY")
                        .help("Currency code (USD, EUR, BTC) for non-interactive mode")
                        .required(false),
                )
                .arg(
                    Arg::new("amount")
                        .long("amount")
                        .value_name("AMOUNT")
                        .help("Decimal amount for non-interactive mode")
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
}

#[cfg(test)]
mod tests {
    use super::TransactionCommandBuilder;

    #[test]
    fn transaction_builder_registers_deposit_and_withdraw() {
        let cmd = TransactionCommandBuilder::new()
            .deposit()
            .withdraw()
            .build();
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "deposit"));
        assert!(cmd.get_subcommands().any(|c| c.get_name() == "withdraw"));
    }

    #[test]
    fn transaction_deposit_parses_confirm_keyword() {
        let cmd = TransactionCommandBuilder::new().deposit().build();
        let matches = cmd
            .try_get_matches_from(["transaction", "deposit", "--confirm-protected", "keyword"])
            .expect("deposit should parse");
        let sub = matches
            .subcommand_matches("deposit")
            .expect("deposit subcommand");
        assert_eq!(
            sub.get_one::<String>("confirm-protected")
                .map(String::as_str),
            Some("keyword")
        );
    }

    #[test]
    fn transaction_non_interactive_fields_parse() {
        let cmd = TransactionCommandBuilder::new().deposit().build();
        let matches = cmd
            .try_get_matches_from([
                "transaction",
                "deposit",
                "--account",
                "main",
                "--currency",
                "USD",
                "--amount",
                "100.25",
            ])
            .expect("deposit should parse non-interactive fields");
        let sub = matches
            .subcommand_matches("deposit")
            .expect("deposit subcommand");
        assert_eq!(
            sub.get_one::<String>("account").map(String::as_str),
            Some("main")
        );
        assert_eq!(
            sub.get_one::<String>("currency").map(String::as_str),
            Some("USD")
        );
        assert_eq!(
            sub.get_one::<String>("amount").map(String::as_str),
            Some("100.25")
        );
    }
}
