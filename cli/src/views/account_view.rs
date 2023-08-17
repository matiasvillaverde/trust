use model::{Account, AccountBalance};
use tabled::settings::style::Style;
use tabled::Table;
use tabled::Tabled;

#[derive(Tabled)]
pub struct AccountView {
    pub name: String,
    pub description: String,
    pub env: String,
}

impl AccountView {
    fn new(account: Account) -> AccountView {
        AccountView {
            name: account.name,
            description: account.description,
            env: account.environment.to_string(),
        }
    }

    pub fn display_account(a: Account) {
        println!();
        println!("Account: {}", a.id);
        AccountView::display_accounts(vec![a]);
        println!();
    }

    pub fn display_accounts(accounts: Vec<Account>) {
        let views: Vec<AccountView> = accounts.into_iter().map(AccountView::new).collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{}", table);
    }
}

#[derive(Tabled)]
pub struct AccountBalanceView {
    pub account_name: String,
    pub total_balance: String,
    pub total_available: String,
    pub total_in_trade: String,
    pub taxed: String,
    pub currency: String,
}

impl AccountBalanceView {
    fn new(balance: AccountBalance, account_name: &str) -> AccountBalanceView {
        AccountBalanceView {
            account_name: crate::views::uppercase_first(account_name),
            total_balance: balance.total_balance.to_string(),
            total_available: balance.total_available.to_string(),
            total_in_trade: balance.total_in_trade.to_string(),
            taxed: balance.taxed.to_string(),
            currency: balance.currency.to_string(),
        }
    }

    pub fn display(balance: AccountBalance, account_name: &str) {
        println!();
        println!("Account balance: {}", balance.id);
        AccountBalanceView::display_balances(vec![balance], account_name);
        println!();
    }

    pub fn display_balances(balances: Vec<AccountBalance>, account_name: &str) {
        let views: Vec<AccountBalanceView> = balances
            .into_iter()
            .map(|x| AccountBalanceView::new(x, account_name))
            .collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{}", table);
    }
}
