use tabled::settings::style::Style;
use tabled::Table;
use tabled::Tabled;
use model::{Account, AccountOverview};

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
pub struct AccountOverviewView {
    pub account_name: String,
    pub total_balance: String,
    pub total_available: String,
    pub total_in_trade: String,
    pub taxed: String,
    pub currency: String,
}

impl AccountOverviewView {
    fn new(overview: AccountOverview, account_name: &str) -> AccountOverviewView {
        AccountOverviewView {
            account_name: crate::views::uppercase_first(account_name),
            total_balance: overview.total_balance.to_string(),
            total_available: overview.total_available.to_string(),
            total_in_trade: overview.total_in_trade.to_string(),
            taxed: overview.taxed.to_string(),
            currency: overview.currency.to_string(),
        }
    }

    pub fn display(overview: AccountOverview, account_name: &str) {
        println!();
        println!("Account overview: {}", overview.id);
        AccountOverviewView::display_overviews(vec![overview], account_name);
        println!();
    }

    pub fn display_overviews(overviews: Vec<AccountOverview>, account_name: &str) {
        let views: Vec<AccountOverviewView> = overviews
            .into_iter()
            .map(|x| AccountOverviewView::new(x, account_name))
            .collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{}", table);
    }
}