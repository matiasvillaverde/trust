use tabled::settings::style::Style;
use tabled::Table;
use tabled::Tabled;
use trust_model::{Account, AccountOverview};

#[derive(Tabled)]
pub struct AccountView {
    pub name: String,
    pub description: String,
}

impl AccountView {
    fn new(account: Account) -> AccountView {
        AccountView {
            name: account.name,
            description: account.description,
        }
    }

    pub fn display_account(a: Account) {
        AccountView::display_accounts(vec![a]);
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
    pub total_taxable: String,
    pub currency: String,
}

impl AccountOverviewView {
    fn new(overview: AccountOverview, account_name: &str) -> AccountOverviewView {
        AccountOverviewView {
            account_name: crate::views::uppercase_first(account_name),
            total_balance: overview.total_balance.amount.to_string(),
            total_available: overview.total_available.amount.to_string(),
            total_in_trade: overview.total_in_trade.amount.to_string(),
            total_taxable: overview.total_taxable.amount.to_string(),
            currency: overview.currency.to_string(),
        }
    }

    pub fn display(overview: AccountOverview, account_name: &str) {
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
